use crate::apt_hooks::{apt_hooks_status, install_apt_hooks, remove_apt_hooks};
use crate::error::Result;

pub async fn apt_hooks_install() -> Result<()> {
    let _ = ();
    install_apt_hooks()
}

pub async fn apt_hooks_remove() -> Result<()> {
    let _ = ();
    remove_apt_hooks()
}

pub async fn apt_hooks_status_cmd() -> Result<()> {
    let _ = ();
    apt_hooks_status()
}