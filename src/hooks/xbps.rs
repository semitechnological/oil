use crate::error::{OilError, Result};

pub fn install() -> Result<()> {
    Err(OilError::PlatformNotSupported(
        "xbps: no pre-install hook API — use `oil install` host-skip only".into(),
    ))
}

pub fn remove() -> Result<()> {
    println!("xbps: no oil hook to remove");
    Ok(())
}

pub fn status() -> Result<()> {
    println!("xbps: unsupported (Void has no package-manager pre-install hook)");
    Ok(())
}