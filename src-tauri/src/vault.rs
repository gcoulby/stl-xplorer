//! Moving files into the vault. The one rule: a move must never lose a file. Every step that
//! can fail leaves the original in place, and a multi-part set moves as a unit or not at all.

use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::util::numbered_name;

/// Move one file. Tries a rename first; across drives, copies, checks the size, then deletes
/// the original. If the original cannot be deleted the copy is removed again.
pub fn move_file(src: &Path, dst: &Path) -> AppResult<()> {
    if dst.exists() {
        return Err(AppError::Invalid(format!(
            "{} already exists",
            dst.display()
        )));
    }
    match std::fs::rename(src, dst) {
        Ok(()) => return Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(AppError::Io(e));
        }
        Err(_) => {}
    }
    let expected = std::fs::metadata(src)?.len();
    if let Err(e) = std::fs::copy(src, dst) {
        let _ = std::fs::remove_file(dst);
        return Err(e.into());
    }
    let copied = std::fs::metadata(dst).map(|m| m.len()).unwrap_or(u64::MAX);
    if copied != expected {
        let _ = std::fs::remove_file(dst);
        return Err(AppError::Other(format!(
            "Copy of {} was incomplete ({copied} of {expected} bytes)",
            src.display()
        )));
    }
    if let Err(e) = std::fs::remove_file(src) {
        let _ = std::fs::remove_file(dst);
        return Err(AppError::Other(format!(
            "Could not remove the original {}: {e}",
            src.display()
        )));
    }
    Ok(())
}

/// Target names for a set of files moving into `dest`. `stem_len` is how many bytes of each
/// name form the shared stem; on a collision every part gets the same ` (n)` after it.
/// Also returns the suffix number used, if any.
pub fn plan_targets(
    sources: &[PathBuf],
    dest: &Path,
    stem_len: usize,
) -> AppResult<(Vec<PathBuf>, Option<u32>)> {
    let names: Vec<String> = sources
        .iter()
        .map(|s| {
            s.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .ok_or_else(|| AppError::Invalid(format!("{} has no file name", s.display())))
        })
        .collect::<AppResult<_>>()?;
    let free = |cands: &[PathBuf]| cands.iter().all(|c| !c.exists());
    let plain: Vec<PathBuf> = names.iter().map(|n| dest.join(n)).collect();
    if free(&plain) {
        return Ok((plain, None));
    }
    for n in 1..10_000 {
        let cands: Vec<PathBuf> = names
            .iter()
            .map(|name| dest.join(numbered_name(name, stem_len, n)))
            .collect();
        if free(&cands) {
            return Ok((cands, Some(n)));
        }
    }
    Err(AppError::Other(
        "Could not find a free name in the vault".into(),
    ))
}

/// Move every source to its target, rolling back all of them if any one fails.
pub fn move_all(pairs: &[(PathBuf, PathBuf)]) -> AppResult<()> {
    for (src, _) in pairs {
        if !src.is_file() {
            return Err(AppError::NotFound(format!("{} is missing", src.display())));
        }
    }
    let mut done: Vec<&(PathBuf, PathBuf)> = Vec::new();
    for pair in pairs {
        if let Err(e) = move_file(&pair.0, &pair.1) {
            let mut stuck = Vec::new();
            for (src, dst) in done.iter().rev() {
                if move_file(dst, src).is_err() {
                    stuck.push(dst.display().to_string());
                }
            }
            if !stuck.is_empty() {
                return Err(AppError::Other(format!(
                    "{e}. Some parts could not be moved back and are now in the vault: {}",
                    stuck.join(", ")
                )));
            }
            return Err(e);
        }
        done.push(pair);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_single_file_with_collision_suffix() {
        let src_dir = tempfile::tempdir().expect("src");
        let vault = tempfile::tempdir().expect("vault");
        let src = src_dir.path().join("cube.stl");
        std::fs::write(&src, b"mesh").expect("write");
        std::fs::write(vault.path().join("cube.stl"), b"other").expect("write");
        let (targets, n) = plan_targets(std::slice::from_ref(&src), vault.path(), 4).expect("plan");
        assert_eq!(n, Some(1));
        assert_eq!(targets, vec![vault.path().join("cube (1).stl")]);
        move_all(&[(src.clone(), targets[0].clone())]).expect("move");
        assert!(!src.exists());
        assert_eq!(std::fs::read(&targets[0]).expect("read"), b"mesh");
        assert_eq!(
            std::fs::read(vault.path().join("cube.stl")).expect("read"),
            b"other"
        );
    }

    #[test]
    fn multi_part_sets_share_one_suffix() {
        let vault = tempfile::tempdir().expect("vault");
        std::fs::write(vault.path().join("Pack.part2.rar"), b"x").expect("write");
        let srcs = vec![
            PathBuf::from("/d/Pack.part1.rar"),
            PathBuf::from("/d/Pack.part2.rar"),
        ];
        let (t, _) = plan_targets(&srcs, vault.path(), 4).expect("plan");
        assert_eq!(
            t,
            vec![
                vault.path().join("Pack (1).part1.rar"),
                vault.path().join("Pack (1).part2.rar")
            ]
        );
    }

    #[test]
    fn failed_part_rolls_back_the_whole_set() {
        let src_dir = tempfile::tempdir().expect("src");
        let vault = tempfile::tempdir().expect("vault");
        let p1 = src_dir.path().join("P.7z.001");
        let p2 = src_dir.path().join("P.7z.002");
        std::fs::write(&p1, b"one").expect("write");
        std::fs::write(&p2, b"two").expect("write");
        // Make the second move fail: its target already exists.
        std::fs::write(vault.path().join("P.7z.002"), b"blocker").expect("write");
        let pairs = vec![
            (p1.clone(), vault.path().join("P.7z.001")),
            (p2.clone(), vault.path().join("P.7z.002")),
        ];
        assert!(move_all(&pairs).is_err());
        assert_eq!(std::fs::read(&p1).expect("p1 back"), b"one");
        assert_eq!(std::fs::read(&p2).expect("p2 kept"), b"two");
        assert!(!vault.path().join("P.7z.001").exists());
        assert_eq!(
            std::fs::read(vault.path().join("P.7z.002")).expect("blocker"),
            b"blocker"
        );
    }

    #[test]
    fn missing_part_moves_nothing() {
        let src_dir = tempfile::tempdir().expect("src");
        let vault = tempfile::tempdir().expect("vault");
        let p1 = src_dir.path().join("P.part1.rar");
        std::fs::write(&p1, b"one").expect("write");
        let pairs = vec![
            (p1.clone(), vault.path().join("P.part1.rar")),
            (
                src_dir.path().join("P.part2.rar"),
                vault.path().join("P.part2.rar"),
            ),
        ];
        assert!(move_all(&pairs).is_err());
        assert!(p1.exists());
        assert!(!vault.path().join("P.part1.rar").exists());
    }
}
