//! Application asset source.
//!
//! `gpui_kit::assets::Assets` embeds the Lucide icon catalog that
//! `gpui_kit::component::Icon` draws from. Without an `AssetSource` the SVG
//! renderer has no bytes behind those paths, so icon-only buttons and `Icon`
//! elements paint nothing. `AppAssets` delegates to the bundled catalog and
//! adds this crate's own files under `logo/` (embedded at compile time).

use gpui_kit::assets::Assets as KitAssets;
gpui_kit::assets::icon_assets!(
    AppIcons,
    [Code, Download, Files, PanelsTopLeft, Save, Trash, Upload]
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
    use gpui_kit::assets::IconName;

    #[test]
    fn application_icons_resolve_even_outside_the_default_kit_bundle() {
        for icon in [
            IconName::Code,
            IconName::Cpu,
            IconName::Download,
            IconName::Files,
            IconName::PanelsTopLeft,
            IconName::Save,
            IconName::Trash,
            IconName::Upload,
            IconName::Sun,
            IconName::SquareTerminal,
        ] {
            let path = icon.path();
            let bytes = AppAssets
                .load(&path)
                .unwrap()
                .unwrap_or_else(|| panic!("Missing icon: {path}"));
            assert!(bytes.starts_with(b"<svg"), "Invalid SVG: {path}");
        }
    }
}
