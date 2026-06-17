use crate::error::{OilError, Result};

pub fn install() -> Result<()> {
    Err(OilError::PlatformNotSupported(
        "nix: no pre-install hook — use oil install host-skip".into(),
    ))
}

pub fn remove() -> Result<()> {
    println!("nix: no oil hook to remove");
    Ok(())
}

pub fn status() -> Result<()> {
    println!("nix: unsupported");
    Ok(())
}