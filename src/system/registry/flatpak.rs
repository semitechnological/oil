//! Flatpak registry — stub.
//!
//! Flatpak apps are installed via the flatpak daemon, not by downloading
//! and extracting package files. Oil detects flatpak, can search and list
//! apps, but install delegates to `flatpak install`.
use super::PackageIndex;
use crate::error::{Result, OilError};

pub struct FlatpakRegistry;

impl FlatpakRegistry {
    pub async fn load(&self, _client: &reqwest::Client) -> Result<PackageIndex> {
        Err(OilError::PlatformNotSupported(
            "flatpak packages are installed via 'flatpak install', not direct download. \
             Use 'flatpak install <remote> <name>' from your shell."
                .to_string(),
        ))
    }
}
