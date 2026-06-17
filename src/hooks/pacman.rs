use super::core::{install_root_file, oil_binary_for_hooks, remove_root_file, shell_escape, write_executable};
use crate::error::{OilError, Result};
use crate::system_pm::SystemPm;
use crate::ui::dirs;
use std::path::PathBuf;

const HOOK_PATH: &str = "/etc/pacman.d/hooks/oil-cellar.hook";
const WRAPPER_NAME: &str = "oil-hook-pacman";

pub fn install() -> Result<()> {
    ensure_pacman()?;
    let oil_bin = oil_binary_for_hooks()?;
    let oil_dir = dirs::oil_dir()?;
    let wrapper_path = oil_dir.join(WRAPPER_NAME);
    write_executable(
        &wrapper_path,
        &format!(
            "#!/bin/sh\nset -eu\nexec {} __pm-preinstall --pkg \"$1\"\n",
            shell_escape(&oil_bin)
        ),
    )?;

    let hook = format!(
        r#"# oil hooks pacman — PreTransaction per package (~1 oil call/pkg)
[Trigger]
Operation = Install
Type = Package
Target = *

[Action]
Description = oil Cellar guard
When = PreTransaction
NeedsTargets
Exec = {wrapper} %p
"#,
        wrapper = shell_escape(&wrapper_path)
    );

    install_root_file(&PathBuf::from(HOOK_PATH), &hook, &oil_dir)?;
    println!("oil hooks pacman: {HOOK_PATH}");
    Ok(())
}

pub fn remove() -> Result<()> {
    let oil_dir = dirs::oil_dir()?;
    remove_root_file(&PathBuf::from(HOOK_PATH))?;
    let w = oil_dir.join(WRAPPER_NAME);
    if w.exists() {
        let _ = std::fs::remove_file(&w);
    }
    println!("oil hooks pacman: removed");
    Ok(())
}

pub fn status() -> Result<()> {
    let hook = PathBuf::from(HOOK_PATH);
    if hook.is_file() {
        println!("pacman: installed ({})", hook.display());
    } else {
        println!("pacman: not installed");
    }
    Ok(())
}

fn ensure_pacman() -> Result<()> {
    if !cfg!(target_os = "linux") {
        return Err(OilError::PlatformNotSupported("pacman hooks: Linux only".into()));
    }
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| OilError::InstallError(format!("runtime: {e}")))?;
    if rt.block_on(SystemPm::detect()).as_ref() != Some(&SystemPm::Pacman) {
        return Err(OilError::PlatformNotSupported(
            "pacman hooks: host is not Arch-style (pacman)".into(),
        ));
    }
    Ok(())
}