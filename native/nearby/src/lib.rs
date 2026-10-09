pub mod bundle;
pub mod catalog;
pub mod compat;
pub mod content;
pub mod context;
pub mod core_choice;
pub mod core_runtime;
pub mod flow;
pub mod game;
pub mod handheld_link;
pub mod handshake;
pub mod http;
pub mod identity;
pub mod installer;
pub mod manual;
pub mod menu;
pub mod pairing;
pub mod protocol;
pub mod radio;
pub mod readiness;
pub mod session;
pub mod system;

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
