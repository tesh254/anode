//! Application asset source.
//!
//! `gpui_kit::assets::Assets` embeds the Lucide icon catalog that
//! `gpui_kit::component::Icon` draws from. Without an `AssetSource` the SVG
//! renderer has no bytes behind those paths, so icon-only buttons and `Icon`
//! elements paint nothing. `AppAssets` delegates to the bundled catalog and
//! adds this crate's own files under `logo/` (embedded at compile time).

use gpui_kit::assets::Assets as KitAssets;

// The icons Anode uses that are not in GPUI Kit's default bundle. An icon
// missing from both sources resolves to no bytes and paints nothing, which is
// why `every_icon_the_application_uses_resolves` reads the sources and checks
// the whole set rather than trusting this list.
gpui_kit::assets::icon_assets!(
    AppIcons,
    [
        Boxes,
        Clock,
        Code,
        Download,
        FileCode,
        Files,
        Hammer,
        LogIn,
        MessagesSquare,
        Microchip,
        PanelsTopLeft,
        Pin,
        Plug,
        Save,
        SquarePen,
        ScanLine,
        Sparkles,
        SquareCode,
        Terminal,
        Trash,
        Unplug,
        Upload,
        Usb,
        Workflow,
        Wrench,
        Zap
    ]
);
use gpui_kit::gpui::{AssetSource, Result as AssetResult, SharedString};

/// The green wordmark rendered inline; `logo/app-logo-black.svg` is the same
/// path with `#2CA63D` replaced by the near-black used on light surfaces.
pub const LOGO_SVG: &str = include_str!("../assets/logo/app-logo-green.svg");
pub const LOGO_DARK_SVG: &str = include_str!("../assets/logo/app-logo-black.svg");

pub const LOGO_PATH: &str = "logo/app-logo-green.svg";
pub const LOGO_DARK_PATH: &str = "logo/app-logo-black.svg";
pub const MARK_PATH: &str = "logo/mark.svg";

/// App assets = Lucide icons + the Anode logo SVGs.
#[derive(Clone, Copy, Debug, Default)]
pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> AssetResult<Option<std::borrow::Cow<'static, [u8]>>> {
        match path {
            MARK_PATH => Ok(Some(std::borrow::Cow::Borrowed(include_bytes!(
                "../assets/logo/mark.svg"
            )))),
            LOGO_PATH => Ok(Some(std::borrow::Cow::Borrowed(LOGO_SVG.as_bytes()))),
            LOGO_DARK_PATH => Ok(Some(std::borrow::Cow::Borrowed(LOGO_DARK_SVG.as_bytes()))),
            _ => match AppIcons.load(path)? {
                Some(bytes) => Ok(Some(bytes)),
                None => KitAssets.load(path),
            },
        }
    }

    fn list(&self, path: &str) -> AssetResult<Vec<SharedString>> {
        let mut listed = KitAssets.list(path)?;
        listed.extend(AppIcons.list(path)?);
        for logo in [LOGO_PATH, LOGO_DARK_PATH, MARK_PATH].into_iter() {
            if logo.starts_with(path) {
                listed.push(logo.into());
            }
        }
        listed.sort();
        listed.dedup();
        Ok(listed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An icon name resolves to `icons/<kebab-case>.svg`, with each capital
    /// and each run of digits starting a new segment: `SquareTerminal` →
    /// `square-terminal`, `Redo2` → `redo-2`.
    fn icon_path(name: &str) -> String {
        let mut path = String::from("icons/");
        let mut previous_was_digit = false;
        for (index, character) in name.char_indices() {
            let starts_segment = (character.is_ascii_uppercase() && index > 0)
                || (character.is_ascii_digit() && !previous_was_digit && index > 0);
            if starts_segment {
                path.push('-');
            }
            path.extend(character.to_lowercase());
            previous_was_digit = character.is_ascii_digit();
        }
        path.push_str(".svg");
        path
    }

    /// Every `IconName::…` written anywhere in the application.
    ///
    /// Reading the sources keeps this honest: a list maintained by hand goes
    /// stale the moment someone reaches for a new icon, and the failure is
    /// invisible — the icon simply paints nothing.
    fn icons_used_in_sources() -> Vec<String> {
        fn collect(directory: &std::path::Path, pattern: &regex::Regex, found: &mut Vec<String>) {
            let Ok(entries) = std::fs::read_dir(directory) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    collect(&path, pattern, found);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    let source = std::fs::read_to_string(&path).unwrap_or_default();
                    for (_, [name]) in pattern
                        .captures_iter(&source)
                        .map(|capture| capture.extract())
                    {
                        found.push(name.to_string());
                    }
                }
            }
        }
        let pattern = regex::Regex::new(r"IconName::([A-Za-z0-9]+)").unwrap();
        let mut found = Vec::new();
        collect(std::path::Path::new("src"), &pattern, &mut found);
        found.sort();
        found.dedup();
        found
    }

    #[test]
    fn icon_names_map_to_their_asset_paths() {
        assert_eq!(icon_path("Cpu"), "icons/cpu.svg");
        assert_eq!(icon_path("SquareTerminal"), "icons/square-terminal.svg");
        assert_eq!(icon_path("Redo2"), "icons/redo-2.svg");
    }

    #[test]
    fn every_icon_the_application_uses_resolves() {
        let used = icons_used_in_sources();
        assert!(
            used.len() > 20,
            "the source scan found only {} icons, so it is not working",
            used.len()
        );
        for name in used {
            let path = icon_path(&name);
            let bytes = AppAssets.load(&path).ok().flatten().unwrap_or_else(|| {
                panic!(
                    "{name} resolves to nothing at {path}. Add it to the `icon_assets!` list \
                     in this file, or it paints as an empty space."
                )
            });
            assert!(bytes.starts_with(b"<svg"), "{path} is not an SVG");
        }
    }

    #[test]
    fn the_brand_assets_resolve() {
        for path in [LOGO_PATH, LOGO_DARK_PATH, MARK_PATH] {
            let bytes = AppAssets
                .load(path)
                .ok()
                .flatten()
                .unwrap_or_else(|| panic!("missing brand asset: {path}"));
            assert!(bytes.starts_with(b"<svg"), "{path} is not an SVG");
        }
    }
}
