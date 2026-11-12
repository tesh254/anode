//! Anode: a native GPUI port of the Electron MicroPython IDE.

pub mod assets;
pub mod boards;
pub mod device;
pub mod hardware;
pub(crate) mod panels;
pub mod profiles;
pub mod project;
pub mod repl;
pub mod settings;
pub mod theme;
pub mod views;
pub mod workspace_state;

pub use workspace_state::{LogEntry, LogKind, OpenTab, Workspace};
