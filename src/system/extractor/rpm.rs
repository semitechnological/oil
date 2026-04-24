/// Extract an .rpm package to dest_dir.
///
/// Tries multiple extraction strategies in order:
/// 1. `rpm2cpio` + `cpio` (fastest, most common on RPM-based distros)
/// 2. `bsdtar` / `tar` with libarchive support (macOS, Arch, some Debian)
/// 3. `rpm2archive` + `tar` (produces a tar archive from RPM)
///
/// TODO: implement pure-Rust RPM header + cpio parsing as ultimate fallback.
/// The `rpm-rs` crate exists but is not yet mature enough for production use.
use crate::error::{Result, WaxError};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Extract an RPM and return (files, dirs). RPM tracked removal is not yet
/// supported, so empty vecs are returned.
pub fn extract_tracked(path: &Path, dest_dir: &Path) -> Result<(Vec<PathBuf>, Vec<PathBuf>)> {
    extract(path, dest_dir)?;
    Ok((vec![], vec![]))
}

pub fn extract(path: &Path, dest_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dest_dir)?;

    // Strategy 1: rpm2cpio + cpio
    if which_cmd("rpm2cpio") && which_cmd("cpio") {
        return extract_with_rpm2cpio(path, dest_dir);
    }

    // Strategy 2: bsdtar (libarchive can read RPMs directly)
    if which_cmd("bsdtar") {
        return extract_with_bsdtar(path, dest_dir);
    }

    // Strategy 3: rpm2archive + tar
    if which_cmd("rpm2archive") && which_cmd("tar") {
        return extract_with_rpm2archive(path, dest_dir);
    }

    Err(WaxError::InstallError(format!(
        "RPM extraction requires one of the following tool chains:\n\
         • rpm2cpio + cpio   (install on Fedora/RHEL: rpm-cpio / cpio)\n\
         • bsdtar            (install on Debian/Ubuntu: libarchive-tools)\n\
         • rpm2archive + tar (install on Fedora/RHEL: rpm)\n\
         Package: {}",
        path.display()
    )))
}

fn which_cmd(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn extract_with_rpm2cpio(path: &Path, dest_dir: &Path) -> Result<()> {
    let rpm2cpio = Command::new("rpm2cpio")
        .arg(path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| WaxError::InstallError(format!("Failed to spawn rpm2cpio: {}", e)))?;

    let cpio_stdout = rpm2cpio
        .stdout
        .ok_or_else(|| WaxError::InstallError("rpm2cpio stdout not available".to_string()))?;

    let output = Command::new("cpio")
        .args(["-idm", "--no-absolute-filenames"])
        .current_dir(dest_dir)
        .stdin(cpio_stdout)
        .output()
        .map_err(|e| WaxError::InstallError(format!("Failed to run cpio: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WaxError::InstallError(format!(
            "cpio failed: {}",
            stderr.trim()
        )));
    }

    Ok(())
}

fn extract_with_bsdtar(path: &Path, dest_dir: &Path) -> Result<()> {
    let output = Command::new("bsdtar")
        .args(["-xf", &path.to_string_lossy()])
        .current_dir(dest_dir)
        .output()
        .map_err(|e| WaxError::InstallError(format!("Failed to run bsdtar: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WaxError::InstallError(format!(
            "bsdtar failed: {}",
            stderr.trim()
        )));
    }

    Ok(())
}

fn extract_with_rpm2archive(path: &Path, dest_dir: &Path) -> Result<()> {
    // rpm2archive writes <pkg>.tgz in the same directory as the input
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_stem().unwrap_or_default();
    let tgz_path = parent.join(format!("{}.tgz", stem.to_string_lossy()));

    let output = Command::new("rpm2archive")
        .arg(path)
        .output()
        .map_err(|e| WaxError::InstallError(format!("Failed to run rpm2archive: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WaxError::InstallError(format!(
            "rpm2archive failed: {}",
            stderr.trim()
        )));
    }

    if !tgz_path.exists() {
        return Err(WaxError::InstallError(
            "rpm2archive did not produce expected .tgz file".to_string(),
        ));
    }

    let output = Command::new("tar")
        .args(["-xzf", &tgz_path.to_string_lossy()])
        .current_dir(dest_dir)
        .output()
        .map_err(|e| WaxError::InstallError(format!("Failed to run tar: {}", e)))?;

    // Clean up the temporary tgz
    let _ = std::fs::remove_file(&tgz_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WaxError::InstallError(format!(
            "tar failed: {}",
            stderr.trim()
        )));
    }

    Ok(())
}
