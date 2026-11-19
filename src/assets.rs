//! Files built into the launcher: fonts, the brand marks and the icons of known apps.

pub const INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");
pub const INTER_MEDIUM: &[u8] = include_bytes!("../assets/fonts/Inter-Medium.ttf");
pub const INTER_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.ttf");
pub const MONTSERRAT_BOLD: &[u8] = include_bytes!("../assets/fonts/Montserrat-Bold.ttf");

/// The launcher's icon (window, taskbar).
pub const APP_ICON_256: &[u8] = include_bytes!("../assets/icons/app-icon-256.png");

/// The hexagon mark and the `{D.E.V.}` wordmark, for dark and light backgrounds.
pub const MARK_DARK: &[u8] = include_bytes!("../assets/icons/mark-dark.png");
pub const MARK_LIGHT: &[u8] = include_bytes!("../assets/icons/mark-light.png");
pub const WORDMARK_DARK: &[u8] = include_bytes!("../assets/icons/wordmark-dark.png");
pub const WORDMARK_LIGHT: &[u8] = include_bytes!("../assets/icons/wordmark-light.png");

/// The icon of an app the launcher knows by id.
pub fn app_icon_png(id: &str) -> Option<&'static [u8]> {
    match id {
        "draftcraft" => Some(include_bytes!("../assets/apps/draftcraft.png")),
        _ => None,
    }
}

/// RGBA pixels of a PNG.
pub fn decode_png(bytes: &[u8]) -> Option<([usize; 2], Vec<u8>)> {
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).ok()?.to_rgba8();
    Some(([img.width() as usize, img.height() as usize], img.into_raw()))
}
