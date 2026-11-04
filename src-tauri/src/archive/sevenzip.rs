//! 7-Zip console backend for 7z, rar and every multi-part format. 7-Zip finds the other
//! volumes itself, so we only ever pass it the entry point.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use super::{safe_join, sanitize_entry_path, ArchiveReader, Entry, Listing, ProgressFn};
use crate::error::{AppError, AppResult, ArchiveFailure};
use crate::types::SevenZipInfo;
use crate::util::Cancel;

/// A password 7-Zip will reject, so encrypted archives fail fast instead of prompting.
const NO_PASSWORD: &str = "-pstlx-no-password";
const MAX_CONCURRENT: usize = 2;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

struct Slots {
    used: Mutex<usize>,
    cv: Condvar,
}

struct Permit<'a>(&'a Slots);

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        if let Ok(mut used) = self.0.used.lock() {
            *used = used.saturating_sub(1);
        }
        self.0.cv.notify_one();
    }
}

impl Slots {
    fn acquire(&self, cancel: &Cancel) -> AppResult<Permit<'_>> {
        let mut used = self
            .used
            .lock()
            .map_err(|_| AppError::Other("7-Zip slot lock poisoned".into()))?;
        while *used >= MAX_CONCURRENT {
            cancel.check()?;
            used = self
                .cv
                .wait_timeout(used, Duration::from_millis(50))
                .map_err(|_| AppError::Other("7-Zip slot lock poisoned".into()))?
                .0;
        }
        *used += 1;
        Ok(Permit(self))
    }
}

pub struct SevenZip {
    exe: Option<PathBuf>,
    source: String,
    slots: Slots,
}

struct RunOutput {
    code: Option<i32>,
    stderr: String,
}

impl SevenZip {
    pub fn discover() -> Self {
        let (exe, source) = find_binary();
        Self {
            exe,
            source,
            slots: Slots {
                used: Mutex::new(0),
                cv: Condvar::new(),
            },
        }
    }

    #[cfg(test)]
    pub fn with_exe(exe: Option<PathBuf>) -> Self {
        Self {
            exe,
            source: "test".into(),
            slots: Slots {
                used: Mutex::new(0),
                cv: Condvar::new(),
            },
        }
    }

    pub fn available(&self) -> bool {
        self.exe.is_some()
    }

    pub fn info(&self) -> SevenZipInfo {
        let version = self.exe.as_ref().and_then(|exe| {
            let mut cmd = Command::new(exe);
            cmd.stdin(Stdio::null()).stderr(Stdio::null());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(CREATE_NO_WINDOW);
            }
            let out = cmd.output().ok()?;
            let text = String::from_utf8_lossy(&out.stdout);
            text.lines()
                .find(|l| l.contains("7-Zip"))
                .map(|l| l.split(':').next().unwrap_or(l).trim().to_string())
        });
        SevenZipInfo {
            path: self.exe.as_ref().map(|p| p.to_string_lossy().into_owned()),
            source: self.source.clone(),
            version,
        }
    }

    fn run(
        &self,
        args: Vec<OsString>,
        cancel: &Cancel,
        on_stdout: &mut dyn FnMut(&[u8]) -> AppResult<()>,
    ) -> AppResult<RunOutput> {
        let exe = self
            .exe
            .as_ref()
            .ok_or(AppError::Archive(ArchiveFailure::ToolMissing))?;
        let _permit = self.slots.acquire(cancel)?;
        let mut cmd = Command::new(exe);
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd.spawn()?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| AppError::Other("no stdout from 7-Zip".into()))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| AppError::Other("no stderr from 7-Zip".into()))?;
        let err_thread = thread::spawn(move || {
            let mut s = Vec::new();
            let _ = stderr.read_to_end(&mut s);
            String::from_utf8_lossy(&s).into_owned()
        });

        let child = Arc::new(Mutex::new(child));
        let finished = Arc::new(AtomicBool::new(false));
        // Reads on the pipe block, so a separate thread watches for cancellation and kills
        // the child, which then closes stdout and unblocks the read below.
        let killer = {
            let child = child.clone();
            let finished = finished.clone();
            let cancel = cancel.clone();
            thread::spawn(move || {
                while !finished.load(Ordering::SeqCst) {
                    if cancel.is_cancelled() {
                        if let Ok(mut c) = child.lock() {
                            let _ = c.kill();
                        }
                        return;
                    }
                    thread::sleep(Duration::from_millis(40));
                }
            })
        };

        let mut buf = vec![0u8; 256 * 1024];
        let mut failure = None;
        loop {
            match stdout.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if let Err(e) = on_stdout(&buf[..n]) {
                        failure = Some(e);
                        if let Ok(mut c) = child.lock() {
                            let _ = c.kill();
                        }
                        break;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    failure = Some(e.into());
                    break;
                }
            }
        }
        drop(stdout);
        let status = child
            .lock()
            .map_err(|_| AppError::Other("7-Zip child lock poisoned".into()))?
            .wait()?;
        finished.store(true, Ordering::SeqCst);
        let _ = killer.join();
        let stderr = err_thread.join().unwrap_or_default();

        if cancel.is_cancelled() {
            return Err(AppError::Cancelled);
        }
        if let Some(e) = failure {
            return Err(e);
        }
        Ok(RunOutput {
            code: status.code(),
            stderr,
        })
    }
}

fn find_binary() -> (Option<PathBuf>, String) {
    let names: &[&str] = if cfg!(windows) {
        &["7zz.exe", "7z.exe"]
    } else {
        &["7zz", "7z"]
    };
    // Tauri puts sidecars next to the main executable, without the target triple.
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        for n in names.iter().take(1) {
            let p = dir.join(n);
            if p.is_file() {
                return (Some(p), "Bundled".into());
            }
        }
    }
    let path_names: &[&str] = if cfg!(windows) {
        &["7zz.exe", "7z.exe", "7za.exe"]
    } else {
        &["7zz", "7z", "7za"]
    };
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            for n in path_names {
                let p = dir.join(n);
                if p.is_file() {
                    return (Some(p), format!("Found on PATH ({n})"));
                }
            }
        }
    }
    let known: Vec<PathBuf> = if cfg!(windows) {
        ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
            .iter()
            .filter_map(std::env::var_os)
            .map(|d| PathBuf::from(d).join("7-Zip").join("7z.exe"))
            .collect()
    } else {
        [
            "/opt/homebrew/bin/7zz",
            "/usr/local/bin/7zz",
            "/opt/homebrew/bin/7z",
            "/usr/local/bin/7z",
            "/usr/bin/7z",
        ]
        .iter()
        .map(PathBuf::from)
        .collect()
    };
    for p in known {
        if p.is_file() {
            return (Some(p), "Installed copy of 7-Zip".into());
        }
    }
    (None, "Not found".into())
}

pub struct SevenZipReader {
    sz: Arc<SevenZip>,
    archive: PathBuf,
    multi_part: bool,
}

impl SevenZipReader {
    pub fn new(sz: Arc<SevenZip>, archive: PathBuf, multi_part: bool) -> Self {
        Self {
            sz,
            archive,
            multi_part,
        }
    }

    fn base_args(&self, cmd: &str) -> Vec<OsString> {
        vec![
            cmd.into(),
            "-sccUTF-8".into(),
            "-scsUTF-8".into(),
            NO_PASSWORD.into(),
        ]
    }

    fn fail(&self, code: Option<i32>, stdout: &str, stderr: &str) -> AppError {
        let text = format!("{stdout}\n{stderr}");
        let _ = code;
        AppError::Archive(classify_error(&text, self.multi_part))
    }
}

impl ArchiveReader for SevenZipReader {
    fn list(&self) -> AppResult<Listing> {
        let mut args = self.base_args("l");
        args.push("-slt".into());
        args.push("--".into());
        args.push(self.archive.clone().into_os_string());
        let mut out = Vec::new();
        let res = self.sz.run(args, &Cancel::new(), &mut |b| {
            out.extend_from_slice(b);
            Ok(())
        })?;
        let text = String::from_utf8_lossy(&out);
        if res.code.unwrap_or(2) >= 2 {
            return Err(self.fail(res.code, &text, &res.stderr));
        }
        Ok(parse_slt(&text))
    }

    fn read_entry(&self, entry: &str, limit: u64, cancel: &Cancel) -> AppResult<Vec<u8>> {
        let mut args = self.base_args("e");
        for a in ["-so", "-spd", "-bd", "-y", "--"] {
            args.push(a.into());
        }
        args.push(self.archive.clone().into_os_string());
        args.push(entry.into());
        let mut out = Vec::new();
        let res = self.sz.run(args, cancel, &mut |b| {
            if (out.len() + b.len()) as u64 > limit {
                return Err(AppError::Archive(ArchiveFailure::TooLarge(
                    (out.len() + b.len()) as u64,
                )));
            }
            out.extend_from_slice(b);
            Ok(())
        })?;
        if res.code.unwrap_or(2) >= 2 {
            return Err(self.fail(res.code, "", &res.stderr));
        }
        if out.is_empty() && res.stderr.to_lowercase().contains("no files to process") {
            return Err(AppError::NotFound(format!("{entry} is not in the archive")));
        }
        Ok(out)
    }

    fn extract_entries(
        &self,
        entries: &[String],
        dest: &Path,
        progress: ProgressFn<'_>,
        cancel: &Cancel,
    ) -> AppResult<Vec<(String, PathBuf)>> {
        let safe: Vec<&String> = entries
            .iter()
            .filter(|e| sanitize_entry_path(e).as_deref() == Some(e.as_str()))
            .collect();
        if safe.is_empty() {
            return Ok(Vec::new());
        }
        std::fs::create_dir_all(dest)?;
        // A list file avoids the Windows command line length limit on big selections.
        let list_path = dest.join(".stlx-entries.txt");
        {
            let mut f = std::fs::File::create(&list_path)?;
            for e in &safe {
                writeln!(f, "{e}")?;
            }
        }
        let mut args = self.base_args("x");
        let mut out_arg = OsString::from("-o");
        out_arg.push(dest.as_os_str());
        args.push(out_arg);
        for a in ["-spd", "-y", "-aoa", "-bsp1", "-bb0"] {
            args.push(a.into());
        }
        let mut list_arg = OsString::from("-ir-@");
        list_arg.push(list_path.as_os_str());
        args.push(list_arg);
        args.push("--".into());
        args.push(self.archive.clone().into_os_string());

        let mut tail = String::new();
        let res = self.sz.run(args, cancel, &mut |b| {
            let s = String::from_utf8_lossy(b);
            if let Some(p) = last_percent(&s) {
                progress(p);
            }
            tail.push_str(&s);
            if tail.len() > 8192 {
                tail.drain(..tail.len() - 4096);
            }
            Ok(())
        });
        let _ = std::fs::remove_file(&list_path);
        let res = res?;
        if res.code.unwrap_or(2) >= 2 {
            return Err(self.fail(res.code, &tail, &res.stderr));
        }
        let mut done = Vec::new();
        for e in safe {
            if let Some(p) = safe_join(dest, e) {
                if p.is_file() {
                    done.push((e.clone(), p));
                }
            }
        }
        progress(100.0);
        Ok(done)
    }
}

fn last_percent(s: &str) -> Option<f32> {
    let mut found = None;
    for (i, _) in s.match_indices('%') {
        let digits: String = s[..i]
            .chars()
            .rev()
            .take_while(char::is_ascii_digit)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        if let Ok(n) = digits.parse::<f32>() {
            found = Some(n.min(100.0));
        }
    }
    found
}

/// Parse `7z l -slt` output. Archive properties come before the `----------` line (there can
/// be several blocks for split sets), and entries follow it, separated by blank lines.
pub fn parse_slt(text: &str) -> Listing {
    let mut listing = Listing::default();
    let mut in_entries = false;
    let mut cur: Vec<(String, String)> = Vec::new();
    let mut blocks: std::collections::HashMap<String, usize> = Default::default();

    let mut flush = |cur: &mut Vec<(String, String)>, listing: &mut Listing| {
        if cur.is_empty() {
            return;
        }
        let get = |k: &str| {
            cur.iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.as_str())
        };
        if let Some(raw) = get("Path") {
            let attrs = get("Attributes").unwrap_or("");
            let is_dir = get("Folder") == Some("+")
                || attrs
                    .split_whitespace()
                    .next()
                    .is_some_and(|a| a.contains('D'));
            let is_link = get("Symbolic Link").is_some_and(|v| !v.is_empty())
                || attrs.split_whitespace().any(|a| a.starts_with('l'));
            let encrypted = get("Encrypted") == Some("+");
            listing.encrypted |= encrypted;
            if let Some(b) = get("Block").filter(|b| !b.is_empty()) {
                if !is_dir {
                    *blocks.entry(b.to_string()).or_insert(0) += 1;
                }
            }
            listing.entries.push(Entry {
                path: sanitize_entry_path(raw),
                size: get("Size").and_then(|s| s.parse().ok()).unwrap_or(0),
                is_dir,
                encrypted,
                is_link,
            });
        }
        cur.clear();
    };

    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if !in_entries {
            if line == "----------" {
                in_entries = true;
                continue;
            }
            if let Some((k, v)) = line.split_once(" = ") {
                if k == "Solid" && v == "+" {
                    listing.solid = true;
                }
            }
            continue;
        }
        if line.is_empty() {
            flush(&mut cur, &mut listing);
            continue;
        }
        if let Some((k, v)) = line.split_once(" = ") {
            cur.push((k.to_string(), v.to_string()));
        } else if let Some(k) = line.strip_suffix(" =") {
            cur.push((k.to_string(), String::new()));
        }
    }
    flush(&mut cur, &mut listing);
    if blocks.values().any(|&n| n > 1) {
        listing.solid = true;
    }
    listing
}

pub fn classify_error(text: &str, multi_part: bool) -> ArchiveFailure {
    let lower = text.to_lowercase();
    if lower.contains("wrong password") || lower.contains("encrypted archive") {
        return ArchiveFailure::Encrypted;
    }
    if lower.contains("missing volume") {
        let detail = text
            .lines()
            .find(|l| l.to_lowercase().contains("missing volume"))
            .and_then(|l| l.rsplit(':').next())
            .filter(|v| !v.to_lowercase().contains("missing volume"))
            .map(|v| format!("Missing part {}", v.trim()))
            .unwrap_or_else(|| "A part of this archive is missing".into());
        return ArchiveFailure::MissingParts(detail);
    }
    if multi_part
        && (lower.contains("unexpected end of archive")
            || lower.contains("cannot open the file as"))
    {
        return ArchiveFailure::MissingParts(
            "A part of this archive is missing or incomplete".into(),
        );
    }
    if lower.contains("is not archive") || lower.contains("cannot open the file as") {
        return ArchiveFailure::Unsupported("not a readable archive".into());
    }
    if lower.contains("unsupported method") {
        return ArchiveFailure::Unsupported("unsupported compression method".into());
    }
    let first = text
        .lines()
        .map(str::trim)
        .find(|l| {
            let l = l.to_lowercase();
            l.starts_with("error") || l.contains("error:") || l.contains("unexpected end")
        })
        .unwrap_or("unknown error")
        .to_string();
    ArchiveFailure::Corrupt(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOLID: &str = "7-Zip 26.05\n\nListing archive: solid.7z\n\n--\nPath = solid.7z\nType = 7z\nSolid = +\nBlocks = 1\n\n----------\nPath = sub dir\nSize = 0\nAttributes = D\nEncrypted = -\nBlock = \n\nPath = a.stl\nSize = 10\nAttributes = A\nEncrypted = -\nBlock = 0\n\nPath = sub dir\\b é.stl\nSize = 300000\nAttributes = A\nEncrypted = -\nBlock = 0\n\n";

    #[test]
    fn parses_solid_listing() {
        let l = parse_slt(SOLID);
        assert!(l.solid);
        assert!(!l.encrypted);
        assert_eq!(l.entries.len(), 3);
        assert!(l.entries[0].is_dir);
        assert_eq!(l.entries[2].path.as_deref(), Some("sub dir/b é.stl"));
        assert_eq!(l.entries[2].size, 300000);
    }

    #[test]
    fn detects_solid_from_shared_blocks() {
        let text = "--\nPath = x.rar\nType = Rar5\n\n----------\nPath = a.stl\nSize = 1\nBlock = 0\n\nPath = b.stl\nSize = 1\nBlock = 0\n";
        assert!(parse_slt(text).solid);
        let text = "--\nPath = x.rar\n\n----------\nPath = a.stl\nSize = 1\nBlock = 0\n\nPath = b.stl\nSize = 1\nBlock = 1\n";
        assert!(!parse_slt(text).solid);
    }

    #[test]
    fn flags_links_and_encryption() {
        let text =
            "----------\nPath = link.stl\nAttributes = A lrwxrwxrwx\nEncrypted = +\nSize = 3\n";
        let l = parse_slt(text);
        assert!(l.encrypted);
        assert!(l.entries[0].is_link);
    }

    #[test]
    fn classifies_errors() {
        assert_eq!(
            classify_error(
                "ERROR: enc.7z : Cannot open encrypted archive. Wrong password?",
                false
            ),
            ArchiveFailure::Encrypted
        );
        assert!(matches!(
            classify_error("ERRORS:\nUnexpected end of archive", true),
            ArchiveFailure::MissingParts(_)
        ));
        assert!(matches!(
            classify_error("ERRORS:\nUnexpected end of archive", false),
            ArchiveFailure::Corrupt(_)
        ));
        assert!(matches!(
            classify_error(
                "ERROR: junk.7z\nOpen ERROR: Cannot open the file as [7z] archive\nIs not archive",
                false
            ),
            ArchiveFailure::Unsupported(_)
        ));
        assert_eq!(
            classify_error("ERROR: Missing volume : pack.part3.rar", true),
            ArchiveFailure::MissingParts("Missing part pack.part3.rar".into())
        );
    }

    #[test]
    fn parses_progress() {
        assert_eq!(last_percent("  5% 2 - a\r 40% 3 - b"), Some(40.0));
        assert_eq!(last_percent("nothing"), None);
    }

    /// Runs against a real 7-Zip when one is installed; skipped otherwise.
    #[test]
    fn real_7zip_roundtrip() {
        let sz = Arc::new(SevenZip::discover());
        if !sz.available() {
            eprintln!("7-Zip not found, skipping");
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("src");
        std::fs::create_dir_all(src.join("sub dir")).expect("mkdir");
        std::fs::write(src.join("a.stl"), b"solid a").expect("write");
        let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
        let noise: Vec<u8> = (0..200_000)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                (seed >> 24) as u8
            })
            .collect();
        std::fs::write(src.join("sub dir").join("b é.stl"), noise).expect("write");
        let archive = dir.path().join("pack.7z");
        let exe = sz.exe.clone().expect("exe");
        let status = Command::new(exe)
            .args(["a", "-t7z", "-ms=on", "-v64k", "-bso0", "-sccUTF-8"])
            .arg(&archive)
            .arg(src.join("a.stl"))
            .arg(src.join("sub dir"))
            .status()
            .expect("run 7z");
        assert!(status.success());
        let first = dir.path().join("pack.7z.001");
        let r = SevenZipReader::new(sz.clone(), first.clone(), true);
        let l = r.list().expect("list");
        assert!(l.solid);
        let names: Vec<_> = l.entries.iter().filter_map(|e| e.path.clone()).collect();
        assert!(names.contains(&"sub dir/b é.stl".to_string()), "{names:?}");
        let bytes = r
            .read_entry("sub dir/b é.stl", 1 << 20, &Cancel::new())
            .expect("read");
        assert_eq!(bytes.len(), 200_000);
        assert!(matches!(
            r.read_entry("sub dir/b é.stl", 1000, &Cancel::new()),
            Err(AppError::Archive(ArchiveFailure::TooLarge(_)))
        ));
        let out = dir.path().join("out");
        let done = r
            .extract_entries(
                &["a.stl".into(), "sub dir/b é.stl".into()],
                &out,
                &mut |_| {},
                &Cancel::new(),
            )
            .expect("extract");
        assert_eq!(done.len(), 2);
        assert_eq!(std::fs::read(out.join("a.stl")).expect("read"), b"solid a");

        // Remove a middle volume and the set must report missing parts.
        let vols: Vec<_> = std::fs::read_dir(dir.path())
            .expect("readdir")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.to_string_lossy().contains("pack.7z.0"))
            .collect();
        assert!(
            vols.len() >= 3,
            "expected several volumes, got {}",
            vols.len()
        );
        std::fs::remove_file(dir.path().join("pack.7z.002")).expect("rm");
        assert!(matches!(
            r.list(),
            Err(AppError::Archive(ArchiveFailure::MissingParts(_)))
        ));
    }
}
