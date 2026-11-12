pub mod arcade;
pub mod bundle;
pub mod catalog;
pub mod compat;
pub mod configuration;
pub mod content;
pub mod context;
pub mod core_choice;
pub mod core_runtime;
pub mod flow;
pub mod game;
pub mod handheld_link;
pub mod handshake;
pub mod home;
pub mod http;
pub mod identity;
pub mod installer;
pub mod launch;
pub mod launcher;
pub mod manual;
pub mod menu;
pub mod netplay_compression;
pub mod network;
pub mod owned_core;
pub mod pairing;
pub(crate) mod platform;
pub mod protocol;
pub mod purpose;
pub mod radio;
pub mod readiness;
pub mod saves;
pub mod session;
pub mod system;
pub mod updates;
pub mod wifi;

#[cfg(test)]
mod contracts;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;

pub fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
