use crate::error::Result;
use crate::hooks::{self, HookPm};

pub async fn hooks_install(pm: Option<&str>) -> Result<()> {
    let _ = ();
    match pm {
        Some(name) => HookPm::from_cli(name)?.install(),
        None => hooks::install_detected(),
    }
}

pub async fn hooks_remove(pm: &str) -> Result<()> {
    let _ = ();
    HookPm::from_cli(pm)?.remove()
}

pub async fn hooks_status(pm: Option<&str>) -> Result<()> {
    let _ = ();
    match pm {
        Some(name) => HookPm::from_cli(name)?.status(),
        None => hooks::status_all(),
    }
}