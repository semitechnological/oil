use super::core::{install_root_file, oil_binary_for_hooks, remove_root_file, shell_escape, write_executable};
use crate::error::{OilError, Result};
use crate::system_pm::SystemPm;
use crate::ui::dirs;
use std::path::{Path, PathBuf};

const HOOKS_DIR: &str = "/etc/apt/apt.conf.d";
const HOOK_FILE: &str = "99oil-dpkg-hooks";
const WRAPPER_NAME: &str = "oil-hook-apt";

pub fn install() -> Result<()> {
    ensure_apt()?;
    let oil_bin = oil_binary_for_hooks()?;
    let oil_dir = dirs::oil_dir()?;
    fs::create_dir_all(&oil_dir)?;

    let wrapper_path = oil_dir.join(WRAPPER_NAME);
    write_executable(&wrapper_path, &wrapper_script(&oil_bin))?;

    let conf_path = PathBuf::from(HOOKS_DIR).join(HOOK_FILE);
    if !Path::new(HOOKS_DIR).exists() {
        return Err(OilError::InstallError(format!(
            "{HOOKS_DIR} missing (need Debian/Ubuntu + root)"
        )));
    }
    install_root_file(&conf_path, &apt_conf(&wrapper_path), &oil_dir)?;
    println!("oil hooks apt: {}", conf_path.display());
    Ok(())
}

pub fn remove() -> Result<()> {
    let oil_dir = dirs::oil_dir()?;
    remove_root_file(&PathBuf::from(HOOKS_DIR).join(HOOK_FILE))?;
    let w = oil_dir.join(WRAPPER_NAME);
    if w.exists() {
        let _ = std::fs::remove_file(&w);
    }
    println!("oil hooks apt: removed");
    Ok(())
}

pub fn status() -> Result<()> {
    let conf = PathBuf::from(HOOKS_DIR).join(HOOK_FILE);
    let w = dirs::oil_dir()?.join(WRAPPER_NAME);
    if conf.is_file() && w.is_file() {
        println!("apt: installed ({})", conf.display());
    } else {
        println!("apt: not installed");
    }
    Ok(())
}

fn ensure_apt() -> Result<()> {
    if !cfg!(target_os = "linux") {
        return Err(OilError::PlatformNotSupported("apt hooks: Linux only".into()));
    }
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| OilError::InstallError(format!("runtime: {e}")))?;
    if rt.block_on(SystemPm::detect()).as_ref() != Some(&SystemPm::Apt) {
        return Err(OilError::PlatformNotSupported(
            "apt hooks: host is not Debian/Ubuntu (apt)".into(),
        ));
    }
    Ok(())
}

fn wrapper_script(oil_bin: &Path) -> String {
    let oil = shell_escape(oil_bin);
    format!(
        r#"#!/bin/sh
# oil hooks apt — DPkg::Pre-Install-Pkgs (light: dpkg-deb + one oil call per .deb)
set -eu
OIL_BIN={oil}
for deb in "$@"; do
  [ -f "$deb" ] || continue
  pkg="$(dpkg-deb -f "$deb" Package 2>/dev/null || true)"
  [ -n "$pkg" ] || continue
  "$OIL_BIN" __pm-preinstall --pkg "$pkg" || exit 1
done
"#
    )
}

fn apt_conf(wrapper: &Path) -> String {
    format!(
        "// oil hooks apt\nDPkg::Pre-Install-Pkgs {{ \"{}\"; }};\n",
        shell_escape(wrapper)
    )
}

use std::fs;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apt_conf_names_dpkg_hook() {
        let c = apt_conf(Path::new("/home/u/.oil/oil-hook-apt"));
        assert!(c.contains("DPkg::Pre-Install-Pkgs"));
    }
}