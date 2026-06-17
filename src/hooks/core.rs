use crate::error::{OilError, Result};
use crate::install::InstallState;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

pub fn pm_preinstall_check_packages(package_names: &[String]) -> Result<()> {
    if package_names.is_empty() {
        return Ok(());
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| OilError::InstallError(format!("runtime: {e}")))?;

    rt.block_on(async {
        let state = InstallState::new()?;
        state.sync_from_cellar().await.ok();
        let installed = state.load().await?;
        let mut blocked = Vec::new();
        for name in package_names {
            if cellar_has_package(name, &installed) {
                blocked.push(name.clone());
            }
        }
        if blocked.is_empty() {
            return Ok(());
        }
        for pkg in &blocked {
            eprintln!(
                "oil: refusing host package manager install of '{}' — already in oil Cellar (oil uninstall {})",
                pkg, pkg
            );
        }
        Err(OilError::InstallError(
            "host install blocked for oil-managed package(s)".into(),
        ))
    })
}

pub fn cellar_has_package(
    name: &str,
    installed: &std::collections::HashMap<String, crate::install::InstalledPackage>,
) -> bool {
    if installed.contains_key(name) {
        return true;
    }
    installed.keys().any(|k| k.eq_ignore_ascii_case(name))
}

pub fn oil_binary_for_hooks() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("OIL_HOOK_BIN") {
        let path = PathBuf::from(&p);
        if path.is_file() {
            return Ok(path);
        }
    }
    if let Ok(p) = std::env::var("OIL_APT_HOOK_BIN") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Ok(path);
        }
    }
    let self_exe = std::env::current_exe()
        .map_err(|e| OilError::InstallError(format!("current_exe: {e}")))?;
    if self_exe.is_file() {
        return Ok(self_exe);
    }
    Err(OilError::InstallError(
        "cannot resolve oil binary for hooks".into(),
    ))
}

pub fn shell_escape(path: &Path) -> String {
    let s = path.to_string_lossy();
    if s.chars().all(|c| c.is_ascii_alphanumeric() || "/._-+:=".contains(c)) {
        s.into_owned()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

pub fn write_executable(path: &Path, body: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, body)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

pub fn install_root_file(path: &Path, content: &str, tmp_dir: &Path) -> Result<()> {
    if nix::unistd::geteuid().is_root() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
        return Ok(());
    }
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("oil-hook.tmp");
    let tmp = tmp_dir.join(file_name);
    fs::write(&tmp, content)?;
    let status = StdCommand::new("sudo")
        .args(["cp", tmp.to_str().unwrap_or(""), path.to_str().unwrap_or("")])
        .status()
        .map_err(|e| OilError::InstallError(format!("sudo: {e}")))?;
    if !status.success() {
        return Err(OilError::InstallError(format!(
            "failed to install {}",
            path.display()
        )));
    }
    let _ = fs::remove_file(&tmp);
    Ok(())
}

pub fn remove_root_file(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    if nix::unistd::geteuid().is_root() {
        fs::remove_file(path)?;
        return Ok(());
    }
    let status = StdCommand::new("sudo")
        .args(["rm", "-f", path.to_str().unwrap_or("")])
        .status()
        .map_err(|e| OilError::InstallError(format!("sudo: {e}")))?;
    if !status.success() {
        return Err(OilError::InstallError(format!(
            "failed to remove {}",
            path.display()
        )));
    }
    Ok(())
}

/// One-line preinstall helper for hook scripts (apt .deb loop, pacman %p, etc.).
pub fn preinstall_shell_fragment(oil_bin: &Path) -> String {
    let oil = shell_escape(oil_bin);
    format!("{oil} __pm-preinstall --pkg")
}