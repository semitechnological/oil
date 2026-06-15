use crate::error::Result;
use std::path::PathBuf;

/// Print the shell command to add oil's bin directory to PATH.
/// Run: eval "$(oil path)"
pub fn oil_path() -> Result<()> {
    println!("export PATH=\"{bin}:$PATH\"", bin = oil_bin_dir().display());
    Ok(())
}

/// Determine oil's bin directory (where binaries are linked after install).
pub fn oil_bin_dir() -> PathBuf {
    crate::system::installer::SystemInstaller::install_prefix().join("bin")
}

/// If running as root, link oil itself into /usr/local/bin so it's in PATH.
pub fn ensure_self_linked() {
    use std::path::Path;
    if !nix::unistd::getuid().is_root() {
        return;
    }
    let target = Path::new("/usr/local/bin/oil");
    if target.exists() {
        return;
    }
    let self_exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return,
    };
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::remove_file(target);
    #[cfg(unix)]
    let _ = std::os::unix::fs::symlink(&self_exe, target);
}
