//! Anode entry point.
//!
//! Bootstrap translated from the Electron `main.ts` + `src/main.tsx`: the
//! process *is* the app now — no main/renderer split, no IPC.

use anode::workspace_state::Workspace;

use gpui_kit::{actions, AppContext as _, WindowOptions};

actions!(anode, [Quit, Save, SaveAll, Run, Stop, Upload]);

/// The app icon PNG the build script rasterized from `assets/app-icon.svg`
/// (the Electron build's `public/app-icon/macos/icon.icns` equivalent).
const APP_ICON_PNG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/app-icon.png"));

fn main() {
    gpui_kit::application()
        .with_assets(anode::assets::AppAssets)
        .run(|cx| {
            // MUST come before anything else.
            gpui_kit::init(cx);

            // Dock/app icon (was `getIconPath()` + BrowserWindow icon). macOS takes
            // the icon at runtime; Linux/Windows windows take it per-window via
            // WindowOptions below.
            #[cfg(target_os = "macos")]
            set_macos_app_icon();

            // Bind keys before any menu construction reads the keymap.
            cx.bind_keys([
                gpui_kit::KeyBinding::new("cmd-q", Quit, None),
                gpui_kit::KeyBinding::new("cmd-s", Save, None),
                gpui_kit::KeyBinding::new("shift-cmd-s", SaveAll, None),
                gpui_kit::KeyBinding::new("cmd-r", Run, None),
                #[cfg(target_os = "macos")]
                gpui_kit::KeyBinding::new("shift-cmd-u", Upload, None),
                #[cfg(not(target_os = "macos"))]
                gpui_kit::KeyBinding::new("ctrl-u", Upload, None),
            ]);

            // Match the Electron app's system appearance preference.
            let mode = gpui_kit::component::ThemeMode::from(cx.window_appearance());
            gpui_kit::component::Theme::change(mode, None, cx);
            anode::theme::apply(cx);

            cx.on_action(quit);
            register_workspace_actions(cx);

            let bounds = gpui_kit::Bounds::<gpui_kit::Pixels> {
                origin: gpui_kit::Point::default(),
                size: gpui_kit::Size {
                    width: gpui_kit::px(1540.),
                    height: gpui_kit::px(900.),
                },
            };

            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(gpui_kit::WindowBounds::Windowed(bounds)),
                    titlebar: Some(gpui_kit::TitlebarOptions {
                        title: Some("Anode".into()),
                        ..gpui_kit::component::TitleBar::title_bar_options()
                    }),
                    window_min_size: Some(gpui_kit::size(gpui_kit::px(1180.), gpui_kit::px(700.))),
                    ..gpui_kit::component::TitleBar::window_options()
                },
                cx,
                |window, cx| {
                    let workspace = cx.new(|cx| Workspace::new(window, cx));
                    workspace.update(cx, |workspace, cx| workspace.initialize_panels(window, cx));
                    cx.set_global(WorkspaceHandle(workspace.downgrade()));
                    workspace
                },
            )
            .expect("failed to open Anode window");
        });
}

fn quit(_: &Quit, cx: &mut gpui_kit::App) {
    cx.quit();
}

/// Set the Dock / cmd-tab icon from the standalone mark PNG that `build.rs`
/// rasterizes from `assets/app-icon.svg`. Embedded at compile time so the
/// binary is self-contained.
#[cfg(target_os = "macos")]
fn set_macos_app_icon() {
    use objc2::AnyThread as _;

    unsafe {
        // initWithData decodes an image file, not a raw RGBA pixel buffer.
        let data = objc2_foundation::NSData::with_bytes(APP_ICON_PNG);
        let ns_image = objc2_app_kit::NSImage::initWithData(objc2_app_kit::NSImage::alloc(), &data);
        if let Some(ns_image) = ns_image {
            ns_image.setSize(objc2_foundation::NSSize::new(1024., 1024.));
            objc2_app_kit::NSApplication::sharedApplication(
                objc2::MainThreadMarker::new().expect("app icon must be set on the main thread"),
            )
            .setApplicationIconImage(Some(&ns_image));
        }
    }
}

/// Linux/Windows: read the same embedded icon as an RGBA image for the window
/// icon field (X11 reads it directly; Windows uses the embedded resource in
/// release packaging).
#[cfg(not(target_os = "macos"))]
fn window_icon() -> Option<std::sync::Arc<image::RgbaImage>> {
    image::load_from_memory(APP_ICON_PNG)
        .ok()
        .map(|image| std::sync::Arc::new(image.to_rgba8()))
}

/// Handle the editor commands at the app level: find the active window's
/// workspace and run the command on it. (The window-level key contexts
/// dispatch these; the handler lives here because the workspace entity is
/// owned by the window.)
fn register_workspace_actions(cx: &mut gpui_kit::App) {
    cx.on_action(|_: &Save, cx| {
        if let Some(workspace) = cx.try_global::<WorkspaceHandle>() {
            if let Some(entity) = workspace.0.upgrade() {
                entity.update(cx, |workspace, cx| {
                    workspace.save_current_file();
                    cx.notify();
                });
            }
        }
    });
    cx.on_action(|_: &SaveAll, cx| {
        if let Some(workspace) = cx.try_global::<WorkspaceHandle>() {
            if let Some(entity) = workspace.0.upgrade() {
                entity.update(cx, |workspace, cx| {
                    workspace.save_all_files();
                    cx.notify();
                });
            }
        }
    });
    cx.on_action(|_: &Run, cx| {
        if let Some(workspace) = cx.try_global::<WorkspaceHandle>() {
            if let Some(entity) = workspace.0.upgrade() {
                entity.update(cx, |workspace, cx| {
                    workspace.run_code();
                    cx.notify();
                });
            }
        }
    });
    cx.on_action(|_: &Stop, cx| {
        if let Some(workspace) = cx.try_global::<WorkspaceHandle>() {
            if let Some(entity) = workspace.0.upgrade() {
                entity.update(cx, |workspace, cx| {
                    workspace.stop_code();
                    cx.notify();
                });
            }
        }
    });
    cx.on_action(|_: &Upload, cx| {
        if let Some(workspace) = cx.try_global::<WorkspaceHandle>() {
            if let Some(entity) = workspace.0.upgrade() {
                entity.update(cx, |workspace, cx| {
                    if workspace.is_arduino() {
                        workspace.arduino_upload();
                    } else {
                        workspace.upload_project();
                    }
                    cx.notify();
                });
            }
        }
    });
}

/// Global holding the workspace entity handle so app-level action listeners
/// can reach it.
struct WorkspaceHandle(gpui_kit::WeakEntity<Workspace>);

impl gpui_kit::Global for WorkspaceHandle {}
