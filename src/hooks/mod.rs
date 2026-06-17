mod apt;
mod apk;
mod brew;
mod core;
mod dnf;
mod pacman;
mod nix;
mod xbps;

pub use core::pm_preinstall_check_packages;

use crate::error::{OilError, Result};
use crate::system_pm::SystemPm;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookPm {
    Apt,
    Apk,
    Dnf,
    Pacman,
    Xbps,
    Brew,
    Nix,
}

impl HookPm {
    pub fn from_cli(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "apt" => Ok(Self::Apt),
            "apk" => Ok(Self::Apk),
            "dnf" | "yum" => Ok(Self::Dnf),
            "pacman" => Ok(Self::Pacman),
            "xbps" => Ok(Self::Xbps),
            "brew" => Ok(Self::Brew),
            "nix" => Ok(Self::Nix),
            _ => Err(OilError::InvalidInput(format!(
                "unknown hooks target '{s}' (try: apt, dnf, pacman, apk, xbps, brew, nix)"
            ))),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Apt => "apt",
            Self::Apk => "apk",
            Self::Dnf => "dnf",
            Self::Pacman => "pacman",
            Self::Xbps => "xbps",
            Self::Brew => "brew",
            Self::Nix => "nix",
        }
    }

    pub fn install(self) -> Result<()> {
        match self {
            Self::Apt => apt::install(),
            Self::Apk => apk::install(),
            Self::Dnf => dnf::install(),
            Self::Pacman => pacman::install(),
            Self::Xbps => xbps::install(),
            Self::Brew => brew::install(),
            Self::Nix => nix::install(),
        }
    }

    pub fn remove(self) -> Result<()> {
        match self {
            Self::Apt => apt::remove(),
            Self::Apk => apk::remove(),
            Self::Dnf => dnf::remove(),
            Self::Pacman => pacman::remove(),
            Self::Xbps => xbps::remove(),
            Self::Brew => brew::remove(),
            Self::Nix => nix::remove(),
        }
    }

    pub fn status(self) -> Result<()> {
        match self {
            Self::Apt => apt::status(),
            Self::Apk => apk::status(),
            Self::Dnf => dnf::status(),
            Self::Pacman => pacman::status(),
            Self::Xbps => xbps::status(),
            Self::Brew => brew::status(),
            Self::Nix => nix::status(),
        }
    }
}

pub fn install_detected() -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| OilError::InstallError(format!("runtime: {e}")))?;
    let pm = rt
        .block_on(SystemPm::detect())
        .ok_or_else(|| OilError::PlatformNotSupported("no host package manager detected".into()))?;
    let hook = match pm {
        SystemPm::Apt => HookPm::Apt,
        SystemPm::Apk => HookPm::Apk,
        SystemPm::Dnf | SystemPm::Yum => HookPm::Dnf,
        SystemPm::Pacman => HookPm::Pacman,
        SystemPm::Xbps => HookPm::Xbps,
        SystemPm::Brew => HookPm::Brew,
        SystemPm::Nix => HookPm::Nix,
        _ => {
            return Err(OilError::PlatformNotSupported(format!(
                "oil hooks install: no hook backend for {}",
                pm.name()
            )));
        }
    };
    hook.install()
}

pub fn status_all() -> Result<()> {
    for pm in [
        HookPm::Apt,
        HookPm::Dnf,
        HookPm::Pacman,
        HookPm::Apk,
        HookPm::Xbps,
        HookPm::Brew,
        HookPm::Nix,
    ] {
        pm.status()?;
    }
    Ok(())
}