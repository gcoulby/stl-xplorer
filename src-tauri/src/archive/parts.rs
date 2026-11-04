//! Groups the files in one folder into archive sets, so multi-part archives show up as one
//! item and gaps in the numbering are caught before 7-Zip is ever run.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct ArchiveSet {
    /// zip, 7z or rar. `None` for bare `.001` sets, whose format must be sniffed.
    pub kind: Option<&'static str>,
    /// Display name, e.g. `Dragons.rar`.
    pub name: String,
    /// The file name prefix shared by every part, used when renaming the whole set.
    pub base: String,
    /// What to hand to the backend. May not exist if that part is missing.
    pub entry: PathBuf,
    pub entry_present: bool,
    /// Parts present on disk, in volume order.
    pub parts: Vec<PathBuf>,
    pub multi_part: bool,
    /// 1-based numbers of parts known to be missing.
    pub missing: Vec<u32>,
    /// Highest part number we can infer.
    pub expected: u32,
}

impl ArchiveSet {
    pub fn missing_detail(&self) -> Option<String> {
        match self.missing.as_slice() {
            [] => None,
            [n] => Some(format!("Missing part {n} of {}", self.expected)),
            many => Some(format!(
                "Missing parts {} of {}",
                many.iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
                self.expected
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Scheme {
    /// name.part1.rar, name.part01.rar ...
    PartRar,
    /// name.rar + name.r00, name.r01 ...
    OldRar,
    /// name.zip + name.z01, name.z02 ... (the .zip holds the last segment)
    SpannedZip,
    /// name.7z.001, name.zip.001, name.rar.001, bare name.001
    Split(Option<&'static str>),
    Single(&'static str),
}

#[derive(Debug)]
struct Piece {
    scheme: Scheme,
    base: String,
    /// Volume number in the set's own order. 0 for heads whose position is decided later.
    n: u32,
    head: bool,
    digits: usize,
}

fn trailing_number(s: &str, min: usize, max: usize) -> Option<(u32, usize)> {
    let len = s.len();
    if !(min..=max).contains(&len) || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok().map(|n| (n, len))
}

fn strip_suffix_ci<'a>(name: &'a str, suffix: &str) -> Option<&'a str> {
    let n = name.len();
    let s = suffix.len();
    if n > s && name.is_char_boundary(n - s) && name[n - s..].eq_ignore_ascii_case(suffix) {
        Some(&name[..n - s])
    } else {
        None
    }
}

fn classify(name: &str) -> Option<Piece> {
    let piece = |scheme, base: &str, n, head, digits| {
        Some(Piece {
            scheme,
            base: base.to_string(),
            n,
            head,
            digits,
        })
    };
    if let Some(stem) = strip_suffix_ci(name, ".rar") {
        if let Some((base, part)) = stem.rsplit_once('.') {
            if part.len() > 4 && part[..4].eq_ignore_ascii_case("part") {
                if let Some((n, d)) = trailing_number(&part[4..], 1, 4) {
                    return piece(Scheme::PartRar, base, n, n == 1, d);
                }
            }
        }
        return piece(Scheme::OldRar, stem, 1, true, 0);
    }
    if let Some(stem) = strip_suffix_ci(name, ".zip") {
        return piece(Scheme::SpannedZip, stem, 0, true, 0);
    }
    if let Some(stem) = strip_suffix_ci(name, ".7z") {
        return piece(Scheme::Single("7z"), stem, 1, true, 0);
    }
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() {
        return None;
    }
    let lower = ext.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix('r') {
        if let Some((n, d)) = trailing_number(rest, 2, 3) {
            // .rar is volume 1, so .r00 is volume 2.
            return piece(Scheme::OldRar, stem, n + 2, false, d);
        }
    }
    if let Some(rest) = lower.strip_prefix('z') {
        if let Some((n, d)) = trailing_number(rest, 2, 3) {
            return piece(Scheme::SpannedZip, stem, n, false, d);
        }
    }
    if let Some((n, d)) = trailing_number(ext, 3, 4) {
        for (inner, kind) in [(".7z", "7z"), (".zip", "zip"), (".rar", "rar")] {
            if let Some(base) = strip_suffix_ci(stem, inner) {
                return piece(Scheme::Split(Some(kind)), base, n, n == 1, d);
            }
        }
        return piece(Scheme::Split(None), stem, n, n == 1, d);
    }
    None
}

fn missing_numbers(present: &[u32], expected: u32) -> Vec<u32> {
    (1..=expected).filter(|n| !present.contains(n)).collect()
}

/// Split a folder's files into archive sets and everything else.
pub fn group_archives(files: &[PathBuf]) -> (Vec<ArchiveSet>, Vec<PathBuf>) {
    type Key = (Scheme, String);
    let mut groups: BTreeMap<Key, Vec<(Piece, PathBuf)>> = BTreeMap::new();
    let mut rest = Vec::new();
    for f in files {
        let Some(name) = f.file_name().and_then(|n| n.to_str()) else {
            rest.push(f.clone());
            continue;
        };
        match classify(name) {
            Some(p) => groups
                .entry((p.scheme.clone(), p.base.to_lowercase()))
                .or_default()
                .push((p, f.clone())),
            None => rest.push(f.clone()),
        }
    }

    let mut sets = Vec::new();
    for ((scheme, _), mut pieces) in groups {
        let dir = pieces[0]
            .1
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let base = pieces[0].0.base.clone();
        match scheme {
            Scheme::Single(kind) => {
                for (p, path) in pieces {
                    sets.push(single(kind, &p.base, path));
                }
            }
            Scheme::OldRar | Scheme::SpannedZip if pieces.iter().all(|(p, _)| p.head) => {
                let kind = if scheme == Scheme::OldRar {
                    "rar"
                } else {
                    "zip"
                };
                for (p, path) in pieces {
                    sets.push(single(kind, &p.base, path));
                }
            }
            Scheme::PartRar | Scheme::OldRar => {
                pieces.sort_by_key(|(p, _)| p.n);
                let nums: Vec<u32> = pieces.iter().map(|(p, _)| p.n).collect();
                let expected = nums.iter().copied().max().unwrap_or(1);
                let head = pieces.iter().find(|(p, _)| p.head).map(|(_, f)| f.clone());
                let entry = head.clone().unwrap_or_else(|| {
                    if scheme == Scheme::PartRar {
                        let d = pieces.iter().map(|(p, _)| p.digits).max().unwrap_or(1);
                        dir.join(format!("{base}.part{:0d$}.rar", 1, d = d))
                    } else {
                        dir.join(format!("{base}.rar"))
                    }
                });
                sets.push(ArchiveSet {
                    kind: Some("rar"),
                    name: format!("{base}.rar"),
                    base,
                    entry_present: head.is_some(),
                    entry,
                    parts: pieces.into_iter().map(|(_, f)| f).collect(),
                    multi_part: true,
                    missing: missing_numbers(&nums, expected),
                    expected,
                });
            }
            Scheme::SpannedZip => {
                let head = pieces.iter().find(|(p, _)| p.head).map(|(_, f)| f.clone());
                let mut vols: Vec<(Piece, PathBuf)> =
                    pieces.into_iter().filter(|(p, _)| !p.head).collect();
                vols.sort_by_key(|(p, _)| p.n);
                let max_vol = vols.iter().map(|(p, _)| p.n).max().unwrap_or(0);
                let expected = max_vol + 1;
                let mut nums: Vec<u32> = vols.iter().map(|(p, _)| p.n).collect();
                if head.is_some() {
                    nums.push(expected);
                }
                let mut parts: Vec<PathBuf> = vols.into_iter().map(|(_, f)| f).collect();
                if let Some(h) = &head {
                    parts.push(h.clone());
                }
                sets.push(ArchiveSet {
                    kind: Some("zip"),
                    name: format!("{base}.zip"),
                    entry_present: head.is_some(),
                    entry: head.unwrap_or_else(|| dir.join(format!("{base}.zip"))),
                    base,
                    parts,
                    multi_part: true,
                    missing: missing_numbers(&nums, expected),
                    expected,
                });
            }
            Scheme::Split(kind) => {
                pieces.sort_by_key(|(p, _)| p.n);
                let head = pieces.iter().find(|(p, _)| p.head).map(|(_, f)| f.clone());
                if kind.is_none() && head.is_none() {
                    // A stray `name.002` with no first part is probably not an archive at all.
                    rest.extend(pieces.into_iter().map(|(_, f)| f));
                    continue;
                }
                let nums: Vec<u32> = pieces.iter().map(|(p, _)| p.n).collect();
                let expected = nums.iter().copied().max().unwrap_or(1);
                let d = pieces.iter().map(|(p, _)| p.digits).max().unwrap_or(3);
                let ext = kind.map(|k| format!(".{k}")).unwrap_or_default();
                let entry = head
                    .clone()
                    .unwrap_or_else(|| dir.join(format!("{base}{ext}.{:0d$}", 1, d = d)));
                let name = if kind.is_some() {
                    format!("{base}{ext}")
                } else {
                    base.clone()
                };
                sets.push(ArchiveSet {
                    kind,
                    name,
                    base: format!("{base}{ext}"),
                    entry_present: head.is_some(),
                    entry,
                    parts: pieces.into_iter().map(|(_, f)| f).collect(),
                    multi_part: true,
                    missing: missing_numbers(&nums, expected),
                    expected,
                });
            }
        }
    }
    (sets, rest)
}

fn single(kind: &'static str, base: &str, path: PathBuf) -> ArchiveSet {
    ArchiveSet {
        kind: Some(kind),
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        base: base.to_string(),
        entry: path.clone(),
        entry_present: true,
        parts: vec![path],
        multi_part: false,
        missing: Vec::new(),
        expected: 1,
    }
}

/// Sniff the format of a bare `.001` set from its magic bytes.
pub fn sniff_kind(path: &Path) -> Option<&'static str> {
    use std::io::Read;
    let mut buf = [0u8; 8];
    let mut f = std::fs::File::open(path).ok()?;
    let n = f.read(&mut buf).ok()?;
    let b = &buf[..n];
    if b.starts_with(b"PK\x03\x04") || b.starts_with(b"PK\x07\x08") {
        Some("zip")
    } else if b.starts_with(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
        Some("7z")
    } else if b.starts_with(b"Rar!\x1a\x07") {
        Some("rar")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(|n| PathBuf::from("/dl").join(n)).collect()
    }

    fn one(names: &[&str]) -> ArchiveSet {
        let (sets, rest) = group_archives(&paths(names));
        assert!(rest.is_empty(), "unexpected leftovers {rest:?}");
        assert_eq!(sets.len(), 1, "{sets:?}");
        sets.into_iter().next().expect("one set")
    }

    #[test]
    fn single_archives_and_other_files() {
        let (sets, rest) =
            group_archives(&paths(&["a.zip", "b.7z", "c.rar", "model.stl", "x.txt"]));
        assert_eq!(sets.len(), 3);
        assert!(sets.iter().all(|s| !s.multi_part && s.missing.is_empty()));
        assert_eq!(rest, paths(&["model.stl", "x.txt"]));
    }

    #[test]
    fn part_rar_sets() {
        let s = one(&[
            "Dragons.part2.rar",
            "Dragons.part1.rar",
            "Dragons.part3.rar",
        ]);
        assert_eq!(s.kind, Some("rar"));
        assert_eq!(s.name, "Dragons.rar");
        assert_eq!(s.entry, PathBuf::from("/dl/Dragons.part1.rar"));
        assert_eq!(s.parts.len(), 3);
        assert!(s.missing.is_empty());

        let s = one(&["D.part01.rar", "D.part02.rar"]);
        assert_eq!(s.entry, PathBuf::from("/dl/D.part01.rar"));
        let s = one(&["D.part001.rar"]);
        assert!(s.multi_part);
        assert_eq!(s.expected, 1);
    }

    #[test]
    fn part_rar_gap_and_missing_first() {
        let s = one(&["D.part1.rar", "D.part2.rar", "D.part4.rar", "D.part5.rar"]);
        assert_eq!(s.missing, vec![3]);
        assert_eq!(s.missing_detail().as_deref(), Some("Missing part 3 of 5"));

        let s = one(&["D.part02.rar", "D.part03.rar"]);
        assert!(!s.entry_present);
        assert_eq!(s.entry, PathBuf::from("/dl/D.part01.rar"));
        assert_eq!(s.missing, vec![1]);
    }

    #[test]
    fn old_style_rar() {
        let s = one(&["Pack.r01", "Pack.rar", "Pack.r00"]);
        assert_eq!(s.entry, PathBuf::from("/dl/Pack.rar"));
        assert_eq!(s.parts, paths(&["Pack.rar", "Pack.r00", "Pack.r01"]));
        assert!(s.missing.is_empty());
        assert_eq!(s.expected, 3);

        let s = one(&["Pack.rar", "Pack.r00", "Pack.r02"]);
        assert_eq!(s.missing, vec![3]);
        let s = one(&["Pack.r00", "Pack.r01"]);
        assert!(!s.entry_present);
        assert_eq!(s.missing, vec![1]);
    }

    #[test]
    fn split_7z_zip_and_bare() {
        let s = one(&["M.7z.002", "M.7z.001", "M.7z.003"]);
        assert_eq!(s.kind, Some("7z"));
        assert_eq!(s.name, "M.7z");
        assert_eq!(s.entry, PathBuf::from("/dl/M.7z.001"));
        assert_eq!(s.base, "M.7z");

        let s = one(&["M.zip.001", "M.zip.003"]);
        assert_eq!(s.kind, Some("zip"));
        assert!(s.multi_part);
        assert_eq!(s.missing, vec![2]);

        let s = one(&["M.001", "M.002"]);
        assert_eq!(s.kind, None);
        assert_eq!(s.name, "M");

        let s = one(&["M.7z.002"]);
        assert!(!s.entry_present);
        assert_eq!(s.entry, PathBuf::from("/dl/M.7z.001"));
    }

    #[test]
    fn stray_bare_number_is_not_an_archive() {
        let (sets, rest) = group_archives(&paths(&["log.002"]));
        assert!(sets.is_empty());
        assert_eq!(rest.len(), 1);
    }

    #[test]
    fn spanned_zip() {
        let s = one(&["S.z02", "S.zip", "S.z01"]);
        assert_eq!(s.entry, PathBuf::from("/dl/S.zip"));
        assert_eq!(s.parts, paths(&["S.z01", "S.z02", "S.zip"]));
        assert_eq!(s.expected, 3);
        assert!(s.missing.is_empty());

        let s = one(&["S.z01", "S.z03", "S.zip"]);
        assert_eq!(s.missing, vec![2]);
        assert_eq!(s.missing_detail().as_deref(), Some("Missing part 2 of 4"));

        let s = one(&["S.z01", "S.z02"]);
        assert!(!s.entry_present);
        assert_eq!(s.missing, vec![3]);
    }

    #[test]
    fn case_insensitive_and_separate_sets() {
        let (sets, _) = group_archives(&paths(&[
            "A.PART1.RAR",
            "a.part2.rar",
            "B.part1.rar",
            "B.part2.rar",
            "c.zip",
        ]));
        assert_eq!(sets.len(), 3);
        let a = sets.iter().find(|s| s.base == "A").expect("set A");
        assert_eq!(a.parts.len(), 2);
    }

    #[test]
    fn multiple_missing_detail() {
        let s = one(&["D.part1.rar", "D.part4.rar"]);
        assert_eq!(
            s.missing_detail().as_deref(),
            Some("Missing parts 2, 3 of 4")
        );
    }

    #[test]
    fn sniffs_formats() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = dir.path().join("x.001");
        std::fs::write(&p, [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0, 4]).expect("write");
        assert_eq!(sniff_kind(&p), Some("7z"));
        std::fs::write(&p, b"Rar!\x1a\x07\x01\x00").expect("write");
        assert_eq!(sniff_kind(&p), Some("rar"));
        std::fs::write(&p, b"hello").expect("write");
        assert_eq!(sniff_kind(&p), None);
    }
}
