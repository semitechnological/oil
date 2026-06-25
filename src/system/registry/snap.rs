//! Snap registry — stub.
//!
//! Snaps are installed via snapd, not by downloading and extracting package
//! files. Oil detects snap, can search and list apps, but install delegates
//! to `snap install`.
use super::PackageIndex;
use crate::error::{Result, OilError};

pub struct SnapRegistry;

impl SnapRegistry {
    pub async fn load(&self, _client: &reqwest::Client) -> Result<PackageIndex> {
        Err(OilError::PlatformNotSupported(
            "snap packages are installed via 'snap install', not direct download. \
             Use 'snap install <name>' from your shell."
                .to_string(),
        ))
    }
}
