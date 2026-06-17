use super::core::{oil_binary_for_hooks, remove_root_file, shell_escape, write_executable};
use crate::error::{OilError, Result};
use crate::system_pm::SystemPm;
use crate::ui::dirs;
use std::path::PathBuf;

const HOOK_DIR: &str = "/etc/apk/commit_hooks.d";
const HOOK_SCRIPT: &str = "oil-cellar.sh";

pub fn install() -> Result<()> {
    ensure_apk()?;
    let oil_bin = oil_binary_for_hooks()?;
    let oil_dir = dirs::oil_dir()?;
    let hook_path = PathBuf::from(HOOK_DIR).join(HOOK_SCRIPT);
    if !PathBuf::from(HOOK_DIR).exists() {
        return Err(OilError::InstallError(format!(
            "{HOOK_DIR} missing (apk 2.12+ commit hooks?)"
        )));
    }
    let body = wrapper_script(&oil_bin);
    let tmp = oil_dir.join(HOOK_SCRIPT);
    write_executable(&tmp, &body)?;
    if nix::unistd::geteuid().is_root() {
        write_executable(&hook_path, &body)?;
    } else {
        let status = std::process::Command::new("sudo")
            .args(["cp", tmp.to_str().unwrap_or(""), hook_path.to_str().unwrap_or("")])
            .status()
            .map_err(|e| OilError::InstallError(format!("sudo: {e}")))?;
        if !status.success() {
            return Err(OilError::InstallError("apk hook install failed".into()));
        }
        let _ = std::process::Command::new("sudo")
            .args(["chmod", "+x", hook_path.to_str().unwrap_or("")])
            .status();
    }
    println!("oil hooks apk: {}", hook_path.display());
    Ok(())
}

fn wrapper_script(oil_bin: &std::path::Path) -> String {
    format!(
        r#"#!/bin/sh
# oil hooks apk — commit hook (light: one oil call per package in APK_INSTALLED_PACKAGES)
set -eu
OIL_BIN={oil}
for pkg in $APK_INSTALLED_PACKAGES; do
  name="${{pkg%%=*}}"
  [ -n "$name" ] || continue
  "$OIL_BIN" __pm-preinstall --pkg "$name" || exit 1
done
"#,
        oil = shell_escape(oil_bin)
    )
}

pub fn remove() -> Result<()> {
    remove_root_file(&PathBuf::from(HOOK_DIR).join(HOOK_SCRIPT))?;
    println!("oil hooks apk: removed");
    Ok(())
}

pub fn status() -> Result<()> {
    let hook = PathBuf::from(HOOK_DIR).join(HOOK_SCRIPT);
    if hook.is_file() {
        println!("apk: installed ({})", hook.display());
    } else {
        println!("apk: not installed");
    }
    Ok(())
}

fn ensure_apk() -> Result<()> {
    if !cfg!(target_os = "linux") {
        return Err(OilError::PlatformNotSupported("apk hooks: Linux only".into()));
    }
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| OilError::InstallError(format!("runtime: {e}")))?;
    if rt.block_on(SystemPm::detect()).as_ref() != Some(&SystemPm::Apk) {
        return Err(OilError::PlatformNotSupported(
            "apk hooks: host is not Alpine/Chimera-style (apk)".into(),
        ));
    }
    Ok(())
}