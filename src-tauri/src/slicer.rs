//! Launching slicers. Paths are always separate arguments; nothing goes through a shell.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::{AppError, AppResult};

#[derive(Debug, PartialEq)]
pub enum LaunchMode {
    Together,
    PerFile { fallback: bool },
}

fn command_for(exe: &Path, args: &[String], files: &[PathBuf]) -> Command {
    #[cfg(target_os = "macos")]
    {
        if exe.extension().is_some_and(|e| e == "app") {
            let mut cmd = Command::new("open");
            cmd.arg("-a").arg(exe);
            if args.is_empty() {
                cmd.args(files);
            } else {
                cmd.arg("--args").args(args).args(files);
            }
            return cmd;
        }
    }
    let mut cmd = Command::new(exe);
    cmd.args(args).args(files);
    cmd
}

fn spawn_detached(mut cmd: Command) -> std::io::Result<()> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    // The child is deliberately not waited on, so closing this app leaves the slicer open.
    cmd.spawn().map(drop)
}

fn too_many_args(e: &std::io::Error) -> bool {
    // ERROR_FILENAME_EXCED_RANGE (206) on Windows, E2BIG (7) on unix.
    matches!(e.raw_os_error(), Some(206) | Some(7))
}

/// Launch the slicer with every file at once, or one process per file when asked to (or when
/// the command line turns out to be too long).
pub fn launch(
    exe: &Path,
    args: &[String],
    files: &[PathBuf],
    one_per_file: bool,
) -> AppResult<LaunchMode> {
    if !exe.exists() {
        return Err(AppError::NotFound(format!(
            "Slicer not found at {}",
            exe.display()
        )));
    }
    if files.is_empty() {
        return Err(AppError::Invalid("Nothing to open".into()));
    }
    if !one_per_file {
        match spawn_detached(command_for(exe, args, files)) {
            Ok(()) => return Ok(LaunchMode::Together),
            Err(e) if too_many_args(&e) && files.len() > 1 => {}
            Err(e) => return Err(AppError::Other(format!("Could not start the slicer: {e}"))),
        }
    }
    for f in files {
        spawn_detached(command_for(exe, args, std::slice::from_ref(f)))
            .map_err(|e| AppError::Other(format!("Could not start the slicer: {e}")))?;
    }
    Ok(LaunchMode::PerFile {
        fallback: !one_per_file,
    })
}

/// Known install locations for the presets.
pub fn detect_presets() -> Vec<(String, PathBuf)> {
    let mut found = Vec::new();
    let mut candidates: Vec<(&str, PathBuf)> = Vec::new();
    #[cfg(windows)]
    {
        let roots: Vec<PathBuf> = [
            "ProgramFiles",
            "ProgramW6432",
            "ProgramFiles(x86)",
            "LOCALAPPDATA",
        ]
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .collect();
        for r in &roots {
            candidates.push((
                "Bambu Studio",
                r.join("Bambu Studio").join("bambu-studio.exe"),
            ));
            candidates.push(("OrcaSlicer", r.join("OrcaSlicer").join("orca-slicer.exe")));
            candidates.push((
                "OrcaSlicer",
                r.join("Programs")
                    .join("OrcaSlicer")
                    .join("orca-slicer.exe"),
            ));
            candidates.push((
                "PrusaSlicer",
                r.join("Prusa3D")
                    .join("PrusaSlicer")
                    .join("prusa-slicer.exe"),
            ));
            candidates.push((
                "PrusaSlicer",
                r.join("PrusaSlicer").join("prusa-slicer.exe"),
            ));
        }
    }
    #[cfg(target_os = "macos")]
    {
        let mut roots = vec![PathBuf::from("/Applications")];
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(home).join("Applications"));
        }
        for r in &roots {
            candidates.push(("Bambu Studio", r.join("BambuStudio.app")));
            candidates.push(("Bambu Studio", r.join("Bambu Studio.app")));
            candidates.push(("OrcaSlicer", r.join("OrcaSlicer.app")));
            candidates.push(("PrusaSlicer", r.join("PrusaSlicer.app")));
            candidates.push((
                "PrusaSlicer",
                r.join("Original Prusa Drivers").join("PrusaSlicer.app"),
            ));
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for (name, bin) in [
            ("Bambu Studio", "bambu-studio"),
            ("OrcaSlicer", "orca-slicer"),
            ("PrusaSlicer", "prusa-slicer"),
        ] {
            if let Some(paths) = std::env::var_os("PATH") {
                for d in std::env::split_paths(&paths) {
                    candidates.push((name, d.join(bin)));
                }
            }
        }
    }
    for (name, path) in candidates {
        if path.exists() && !found.iter().any(|(n, _): &(String, PathBuf)| n == name) {
            found.push((name.to_string(), path));
        }
    }
    found
}
