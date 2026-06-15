use crate::error::Result;
use std::path::PathBuf;

/// Print the shell command to add oil's bin directory to PATH.
/// Run: eval "$(oil path)"
pub fn oil_path() -> Result<()> {
    let bins = oil_bin_dirs();
    let path_entries: Vec<String> = bins.iter().map(|b| b.to_string_lossy().to_string()).collect();
    println!("export PATH=\"{path}:$PATH\"", path = path_entries.join(":"));
    Ok(())
}

/// Determine oil's bin directories (where binaries are linked after install).
pub fn oil_bin_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    // System install prefix (e.g. ~/.local/bin or /usr/local/bin)
    dirs.push(crate::system::installer::SystemInstaller::install_prefix().join("bin"));
    // Homebrew-style Cellar bin (e.g. ~/.local/oil/bin)
    if let Ok(prefix) = crate::install::InstallMode::detect().prefix() {
        let cellar_bin = prefix.join("bin");
        if cellar_bin != dirs[0] {
            dirs.push(cellar_bin);
        }
    } else {
        let bp = crate::bottle::homebrew_prefix().join("bin");
        if bp != dirs[0] {
            dirs.push(bp);
        }
    }
    dirs
}

/// Primary bin dir (for system installer wrapper symlinks).
pub fn oil_bin_dir() -> PathBuf {
    oil_bin_dirs().into_iter().next().unwrap_or_else(|| PathBuf::from("/usr/local/bin"))
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
