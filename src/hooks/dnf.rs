use super::core::{install_root_file, oil_binary_for_hooks, remove_root_file, shell_escape};
use crate::error::{OilError, Result};
use crate::system_pm::SystemPm;
use crate::ui::dirs;
use std::path::PathBuf;

const PLUGIN_DIR: &str = "/etc/dnf/plugins";
const PLUGIN_PY: &str = "oil_cellar.py";
const PLUGIN_CONF: &str = "oil_cellar.conf";

pub fn install() -> Result<()> {
    ensure_dnf()?;
    let oil_bin = oil_binary_for_hooks()?;
    let oil_dir = dirs::oil_dir()?;
    let plugin_dir = PathBuf::from(PLUGIN_DIR);
    if !std::path::Path::new(PLUGIN_DIR).exists() && !nix::unistd::geteuid().is_root() {
        return Err(OilError::InstallError(format!(
            "{PLUGIN_DIR} missing — install dnf-plugins-core and retry as root"
        )));
    }

    let py = dnf_plugin_py(&oil_bin);
    let conf = "[main]\nenabled = 1\n";
    install_root_file(&plugin_dir.join(PLUGIN_PY), &py, &oil_dir)?;
    install_root_file(&plugin_dir.join(PLUGIN_CONF), conf, &oil_dir)?;
    println!("oil hooks dnf: {PLUGIN_DIR}/{PLUGIN_PY}");
    Ok(())
}

pub fn remove() -> Result<()> {
    remove_root_file(&PathBuf::from(PLUGIN_DIR).join(PLUGIN_PY))?;
    remove_root_file(&PathBuf::from(PLUGIN_DIR).join(PLUGIN_CONF))?;
    println!("oil hooks dnf: removed");
    Ok(())
}

pub fn status() -> Result<()> {
    let py = PathBuf::from(PLUGIN_DIR).join(PLUGIN_PY);
    if py.is_file() {
        println!("dnf: installed ({})", py.display());
    } else {
        println!("dnf: not installed");
    }
    Ok(())
}

fn ensure_dnf() -> Result<()> {
    if !cfg!(target_os = "linux") {
        return Err(OilError::PlatformNotSupported("dnf hooks: Linux only".into()));
    }
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| OilError::InstallError(format!("runtime: {e}")))?;
    let pm = rt.block_on(SystemPm::detect());
    match pm.as_ref() {
        Some(SystemPm::Dnf) | Some(SystemPm::Yum) => Ok(()),
        _ => Err(OilError::PlatformNotSupported(
            "dnf hooks: host is not Fedora/RHEL-style (dnf/yum)".into(),
        )),
    }
}

fn dnf_plugin_py(oil_bin: &std::path::Path) -> String {
    let oil = shell_escape(oil_bin);
    format!(
        r#"# oil hooks dnf — pre_transaction plugin (light: subprocess per pkg)
import dnf
import dnf.plugin
import subprocess

class Plugin(dnf.plugin.Plugin):
    name = "oil_cellar"

    def __init__(self, base, cli):
        super().__init__(base, cli)
        self.base = base

    def pre_transaction(self):
        ts = self.base.transaction
        if ts is None:
            return
        names = set()
        for t in ts.install_set | ts.upgrade_set:
            names.add(t.name)
        oil = {oil}
        for name in sorted(names):
            r = subprocess.run([oil, "__pm-preinstall", "--pkg", name], check=False)
            if r.returncode != 0:
                raise dnf.exceptions.Error(f"oil blocked install of {{name}}")
"#
    )
}