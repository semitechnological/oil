use crate::error::{OilError, Result};

pub fn install() -> Result<()> {
    Err(OilError::PlatformNotSupported(
        "brew: no supported pre-install hook — oil install already skips if brew has the formula".into(),
    ))
}

pub fn remove() -> Result<()> {
    println!("brew: no oil hook to remove");
    Ok(())
}

pub fn status() -> Result<()> {
    println!("brew: unsupported (Homebrew has no dpkg-style pre-install hook)");
    Ok(())
}