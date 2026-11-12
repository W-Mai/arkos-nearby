//! System settings and foreground resources supplied by the active platform.

use howff_menu::SystemLevel;

pub(crate) use super::arkos::controls::Hardware;
pub(crate) use super::arkos::tools::{
    Prepared, check_tool, check_tool_runtime, prepare_tool, recover_pending,
};

#[derive(Clone, Copy)]
pub(crate) struct Reading {
    pub value: u8,
    pub muted: bool,
}

pub(crate) trait Driver: Send + 'static {
    fn read(&mut self, control: SystemLevel) -> std::result::Result<Reading, String>;
    fn write(&mut self, control: SystemLevel, target: u8) -> std::result::Result<Reading, String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    SystemInfo,
    Files,
}

pub(crate) use super::arkos::frontend::release_uninstalled_frontend;
#[cfg(target_os = "linux")]
pub use super::arkos::frontend::{activate, launch, launch_standalone, listen};
#[cfg(target_os = "linux")]
pub(crate) use super::arkos::frontend::{
    definition, foreground, frontend_active, start_frontend, stop_frontend, validate_frontend,
};
pub(crate) use super::arkos::frontend::{foreground_unit, gui_command, pause_game, resume_game};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerAction {
    Poweroff,
    Reboot,
}

pub(crate) use super::arkos::solo::{Solo, power};
