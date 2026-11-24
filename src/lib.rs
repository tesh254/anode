//! Anode: a native hardware programming IDE built with GPUI Kit.

pub mod agent;
pub mod assets;
pub mod boards;
pub mod device;
pub mod esp;
pub mod hardware;
pub(crate) mod panels;
pub mod profiles;
pub mod project;
pub(crate) mod recovery;
pub mod repl;
pub mod settings;
pub(crate) mod terminal;
pub mod theme;
pub mod toolchain;
pub mod views;
pub mod workspace_state;

pub use panels::{Tool, ToolGroup};
pub use workspace_state::{LogEntry, LogKind, OpenTab, Workspace};
