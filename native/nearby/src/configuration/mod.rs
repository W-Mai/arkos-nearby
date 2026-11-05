pub mod definitions;
pub mod files;
mod paths;
mod resolve;
mod slots;
mod transaction;

pub use paths::ResolvedSavePaths;
pub use resolve::{Environment, Snapshot};
pub use slots::{StartupSlot, StateSlots};
pub const JOURNAL: &str = "/opt/arkos-nearby/configuration-transaction.json";
pub fn recover() -> crate::Result<()> {
    transaction::recover(std::path::Path::new(JOURNAL))
}

#[cfg(test)]
mod tests;
