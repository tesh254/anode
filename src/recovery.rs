//! Recovery guidance is selected from the failure and the affected board's
//! runtime. It never performs a reset, retries a write, or installs firmware.
use crate::boards::{BoardDef, Runtime};

#[derive(Clone, Debug)]
pub(crate) struct RecoveryContext {
    pub port: String,
    pub runtime: Runtime,
    pub board: Option<&'static BoardDef>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecoveryKind {
    General,
    RawRepl,
    PortBusy,
    Disconnected,
    AccessDenied,
    Bootloader,
    DriveMissing,
}

impl RecoveryKind {
    pub fn from_error(error: &str) -> Option<Self> {
        let error = error.to_lowercase();
        if error.contains("raw repl") {
            Some(Self::RawRepl)
        } else if [
            "port in use",
            "board is in use",
            "resource busy",
            "already using this board",
            "in use by another program",
            "exclusively lock",
        ]
        .iter()
        .any(|text| error.contains(text))
        {
            Some(Self::PortBusy)
        } else if error.contains("permission denied") || error.contains("access is denied") {
            Some(Self::AccessDenied)
        } else if [
            "device disconnected",
            "device not configured",
            "no such file or directory",
        ]
        .iter()
        .any(|text| error.contains(text))
        {
            // A missing source file is not evidence of a disconnected board.
            (error.contains("/dev/") || !error.contains("no such file or directory"))
                .then_some(Self::Disconnected)
        } else if ["no serial data received", "wrong boot mode", "no dfu"]
            .iter()
            .any(|text| error.contains(text))
            || (error.contains("failed to connect")
                && ["esp", "chip", "serial", "bootloader"]
                    .iter()
                    .any(|text| error.contains(text)))
        {
            Some(Self::Bootloader)
        } else if error.contains("drive not mounted") || error.contains("bootloader volume") {
            Some(Self::DriveMissing)
        } else {
            None
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::General => "Connect your board",
            Self::RawRepl => "MicroPython did not answer",
            Self::PortBusy => "The serial port is busy",
            Self::Disconnected => "The board disconnected",
            Self::AccessDenied => "Serial access was denied",
            Self::Bootloader => "The bootloader did not answer",
            Self::DriveMissing => "The board's USB drive is missing",
        }
    }

    pub fn explanation(self) -> &'static str {
        match self {
            Self::General => "Work through these checks in order, then retry your original operation.",
            Self::RawRepl => "Anode could not enter the Python interpreter to start this operation. This error alone does not show whether the port is busy or the expected firmware is running.",
            Self::PortBusy => "A serial monitor, REPL, or another device operation may own the port.",
            Self::Disconnected => "The selected connection disappeared. A reset can bring the board back with a different port name.",
            Self::AccessDenied => "The operating system or another application prevented access to this port.",
            Self::Bootloader => "The programming tool could not communicate with the board's firmware loader.",
            Self::DriveMissing => "Anode could not find the USB volume required for this operation.",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RecoveryIssue {
    pub context: RecoveryContext,
    pub kind: RecoveryKind,
    pub detail: String,
}

impl RecoveryIssue {
    pub fn from_error(context: RecoveryContext, error: &str) -> Option<Self> {
        Some(Self {
            context,
            kind: RecoveryKind::from_error(error)?,
            detail: error.to_owned(),
        })
    }
}

impl RecoveryContext {
    pub fn steps(&self, kind: RecoveryKind) -> Vec<&'static str> {
        let mut steps = vec![
            "Close other serial monitors and disconnect any REPL using this board. Wait for any upload or firmware operation to finish.",
            "Check the USB connection with a data-capable cable. Reconnect directly to the computer if a hub is unreliable, then rescan and select the board's current port.",
        ];
        if kind == RecoveryKind::AccessDenied {
            steps.push("Check the port's access permissions and the USB driver recommended by the board maker. On Linux, your account may need membership in the serial-device group.");
        }
        match self.runtime {
            Runtime::MicroPython => {
                steps.push("Start the board normally, without holding a bootloader button. Open the REPL and click its ^C interrupt button twice. Look for a MicroPython banner and the >>> prompt.");
                steps.push("If a program keeps restarting or blocks the prompt, use the board's RESET or RUN button if available, then interrupt it again. Use the maker's safe-mode procedure if normal startup still blocks access.");
                steps.push("Choosing MicroPython in Anode does not install it on the board. If MicroPython never appears, confirm the exact board model and its installed firmware before reviewing firmware recovery.");
            }
            Runtime::CircuitPython => {
                steps.push("Start the board normally and look for the CIRCUITPY drive containing boot_out.txt. A bootloader drive is not CIRCUITPY; reset normally to leave the bootloader.");
                steps.push("Open the REPL and click its ^C interrupt button to stop code.py if needed. Deploy code.py through the CIRCUITPY drive; copying it does not require a MicroPython raw REPL.");
                steps.push("If CIRCUITPY is still missing, follow the maker's CircuitPython troubleshooting instructions before considering a firmware reinstall.");
            }
            Runtime::Arduino => {
                steps.push("Confirm the board model, board package/core, and port in Board setup. Upload the compiled sketch using Upload; an Arduino sketch does not provide a Python >>> prompt.");
                steps.push("Retry Upload after closing serial monitors. Only use a bootloader button sequence when the selected board's upload instructions require it.");
            }
            Runtime::Native => {
                steps.push("Confirm the board target and upload configuration for the project's PlatformIO or ESP-IDF toolchain. Build the firmware, then use Upload; a native application does not provide a Python >>> prompt.");
                steps.push("Close serial monitors before uploading. If the programmer still cannot connect, review the exact board's bootloader instructions.");
            }
        }
        steps
    }

    pub fn firmware_help(&self) -> String {
        let Some(board) = self.board.filter(|board| !board.generic) else {
            return "Identify the exact board model in Board setup before following a bootloader sequence or choosing firmware. USB chip-family detection alone is not enough.".into();
        };
        if !self.runtime.is_interpreted() {
            return board
                .flash
                .preparation()
                .unwrap_or("Use the board maker's upload instructions for the selected target.")
                .into();
        }
        match board.flash.preparation() {
            Some(preparation) => format!(
                "Only if you intend to install firmware: {preparation} Use an image for {}. Installing firmware replaces the runtime and may remove saved files; review the flash confirmation first.",
                board.name,
            ),
            None => "This board has no firmware installation path in Anode. Check its supported runtimes and the board maker's instructions.".into(),
        }
    }

    pub fn documentation(&self) -> &'static str {
        match self.runtime {
            Runtime::MicroPython => "https://docs.micropython.org/en/latest/reference/mpremote.html",
            Runtime::CircuitPython => "https://learn.adafruit.com/welcome-to-circuitpython/troubleshooting",
            Runtime::Arduino => "https://support.arduino.cc/hc/en-us/articles/4403365313810-If-your-sketch-doesn-t-upload",
            Runtime::Native => "https://docs.platformio.org/en/latest/core/userguide/device/cmd_monitor.html",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boards::BOARD_CATALOG;

    #[gpui_kit::test]
    fn a_failed_raw_repl_opens_guidance_and_board_setup_without_touching_hardware(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use crate::{panels::Tool, workspace_state::Workspace};
        use gpui_kit::test::TestWindowExt as _;
        use gpui_kit::{px, size, AppContext as _, Bounds, WindowBounds, WindowOptions};
        let config = tempfile::tempdir().unwrap();
        std::env::set_var("ANODE_CONFIG_DIR", config.path());
        std::env::set_var("ANODE_DISABLE_HARDWARE_PROBE", "1");
        cx.update(gpui_kit::init);
        let (handle, workspace) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        Default::default(),
                        size(px(1180.), px(700.)),
                    ))),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    let workspace = cx.new(|cx| Workspace::new(window, cx));
                    workspace.update(cx, |workspace, cx| {
                        workspace.settings.agent_auto_connect = false;
                        workspace.device_recovery = RecoveryIssue::from_error(
                            RecoveryContext {
                                port: "/dev/anode-test-port".into(),
                                runtime: Runtime::MicroPython,
                                board: None,
                            },
                            "could not enter raw REPL (board may be booting or port in use)",
                        );
                        workspace.initialize_panels(window, cx);
                    });
                    workspace
                },
            )
            .unwrap()
        });
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.click("workspace-recovery-help", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("recovery-done").visible());
            assert!(window.find("recovery-done").bounds().bottom() <= px(700.));
            assert_eq!(window.find("recovery-repl").disabled(), Some(true));
            window.click("recovery-copy", cx);
            window.click("recovery-setup", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update(|cx| {
            assert!(workspace.read(cx).is_tool_visible(Tool::Device));
            assert!(!workspace.read(cx).device_busy);
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("recovery-done").is_none());
            window.click("workspace-recovery-dismiss", cx);
            window.click("device-connection-help", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("recovery-done").visible());
            window.press("escape", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("recovery-done").is_none());
            assert!(window.try_find("workspace-recovery-help").is_none());
        })
        .unwrap();
    }

    #[test]
    fn identifies_connection_failures_without_mislabeling_program_errors() {
        assert_eq!(
            RecoveryKind::from_error(
                "could not enter raw REPL (board may be booting or port in use)"
            ),
            Some(RecoveryKind::RawRepl)
        );
        assert_eq!(
            RecoveryKind::from_error("serial port in use (disconnect REPL first)"),
            Some(RecoveryKind::PortBusy)
        );
        assert_eq!(
            RecoveryKind::from_error("Permission denied: /dev/ttyACM0"),
            Some(RecoveryKind::AccessDenied)
        );
        assert_eq!(
            RecoveryKind::from_error("device disconnected during operation"),
            Some(RecoveryKind::Disconnected)
        );
        assert_eq!(
            RecoveryKind::from_error("Failed to connect: No serial data received"),
            Some(RecoveryKind::Bootloader)
        );
        assert_eq!(
            RecoveryKind::from_error("CIRCUITPY drive not mounted"),
            Some(RecoveryKind::DriveMissing)
        );
        assert_eq!(
            RecoveryKind::from_error("No such file or directory: main.py"),
            None
        );
        assert_eq!(
            RecoveryKind::from_error("OSError: Could not join network"),
            None
        );
        assert_eq!(RecoveryKind::from_error("Failed to connect to WiFi"), None);
        assert_eq!(RecoveryKind::from_error("error: missing semicolon"), None);
    }

    #[test]
    fn every_catalog_runtime_has_guidance_and_compiled_boards_do_not_expect_python() {
        for board in BOARD_CATALOG {
            for &runtime in board.runtimes {
                let context = RecoveryContext {
                    port: String::new(),
                    runtime,
                    board: Some(board),
                };
                let steps = context.steps(RecoveryKind::General).join(" ");
                assert!(steps.contains("USB"));
                assert!(!context.firmware_help().is_empty());
                if !runtime.is_interpreted() {
                    assert!(steps.contains("does not provide a Python"));
                }
                if board.generic {
                    assert!(!context.firmware_help().contains("Hold BOOT"));
                }
            }
        }
    }
}
