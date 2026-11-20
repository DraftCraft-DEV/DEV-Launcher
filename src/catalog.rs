//! The apps the launcher knows.
//!
//! The list ships inside the launcher (`catalog.json` at the repository root) and is refreshed
//! from the same file on the launcher repository's `main` branch, so a new DraftCraft Engineering
//! Ventures app shows up without a launcher release.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::paths;

const EMBEDDED: &str = include_str!("../catalog.json");

/// The launcher's own GitHub repository (`owner/name`), for its self-update and for refreshing
/// this catalog. Not set yet: until it is, the built-in catalog is used and the launcher doesn't
/// update itself.
/// The website of DraftCraft Engineering Ventures and its apps.
pub const WEBSITE: &str = "https://draftcraft.dev";

pub const LAUNCHER_REPO: Option<&str> = Some("DraftCraft-DEV/DEV-Launcher");

/// Where the up-to-date catalog lives, once the launcher has a repository.
pub fn remote_url() -> Option<String> {
    LAUNCHER_REPO.map(|repo| format!("https://raw.githubusercontent.com/{repo}/main/catalog.json"))
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Catalog {
    #[serde(default)]
    pub schema: u32,
    pub apps: Vec<AppDef>,
}

/// Who makes an app. The launcher keeps the two apart: DraftCraft Engineering Ventures' own apps,
/// and open-source apps made by others, listed separately with full credit to their creators.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    /// Made by DraftCraft Engineering Ventures (D.E.V.).
    #[default]
    Dev,
    /// An independent open-source project, not made by D.E.V.
    Outside,
}

/// The people or organization behind an app, and where to find them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Publisher {
    /// As the creators name themselves ("Tangency Labs", "TraceDraw contributors").
    pub name: String,
    /// Their own page: the project's repository or website.
    pub url: String,
}

/// Text in several languages; English is the fallback.
pub type Localized = BTreeMap<String, String>;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AppDef {
    pub id: String,
    pub name: String,
    /// GitHub `owner/name` that publishes the releases.
    pub repo: String,
    /// Whether D.E.V. makes it or it is an outside open-source project.
    #[serde(default)]
    pub origin: Origin,
    /// Who makes it, credited and linked on its page. Required for outside apps.
    #[serde(default)]
    pub publisher: Option<Publisher>,
    /// The app's licence as an SPDX expression ("MIT OR Apache-2.0", "GPL-3.0").
    #[serde(default)]
    pub license: Option<String>,
    /// `#RRGGBB`.
    #[serde(default)]
    pub accent: Option<String>,
    /// Release files are `<asset_prefix>-<os>-<arch>.<ext>`.
    pub asset_prefix: String,
    #[serde(default)]
    pub tagline: Localized,
    #[serde(default)]
    pub description: Localized,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    /// A PNG icon to download for apps whose icon isn't built in.
    #[serde(default)]
    pub icon_url: Option<String>,
    /// Shown as "coming soon": listed, nothing to install yet.
    #[serde(default)]
    pub coming_soon: bool,
    #[serde(default)]
    pub macos: MacDef,
    #[serde(default)]
    pub windows: WinDef,
    #[serde(default)]
    pub linux: LinuxDef,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MacDef {
    pub bundle_id: String,
    /// `DraftCraft.app`
    pub app_name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WinDef {
    /// The program in the portable zip (`DraftCraft.exe`).
    pub exe: String,
    /// The program the setup installs and registers under `App Paths` (`draftcraft.exe`).
    pub installed_exe: String,
    /// `DisplayName` of the Windows Installer product.
    pub display_name: String,
    /// The top folder inside the portable zip, dropped when unpacking.
    #[serde(default)]
    pub zip_root: Option<String>,
    /// Install only the portable zip on Windows. For apps whose setup isn't a Windows Installer
    /// one that takes the launcher's silent options (TraceDraw's is Inno Setup): the launcher
    /// could neither run it quietly nor remove it cleanly, while the zip it handles fully.
    #[serde(default)]
    pub portable_only: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LinuxDef {
    /// Name of the .deb/.rpm/Arch package.
    pub package: String,
    /// The program a package puts on `PATH`.
    pub binary: String,
    /// Desktop entry id (`<id>.desktop`) and icon name.
    pub desktop_id: String,
    /// File name for the AppImage the launcher installs.
    pub appimage_name: String,
}

impl AppDef {
    pub fn tagline(&self, lang: &str) -> &str {
        pick(&self.tagline, lang)
    }

    pub fn description(&self, lang: &str) -> &str {
        pick(&self.description, lang)
    }

    /// An independent open-source app, not made by D.E.V.
    pub fn is_outside(&self) -> bool {
        self.origin == Origin::Outside
    }

    /// The creators' page: their own link, else the app's repository.
    pub fn publisher_url(&self) -> String {
        self.publisher.as_ref().map_or_else(|| format!("https://github.com/{}", self.repo), |p| p.url.clone())
    }

    pub fn accent_rgb(&self) -> Option<[u8; 3]> {
        parse_hex(self.accent.as_deref()?)
    }

    pub fn releases_url(&self) -> String {
        format!("https://github.com/{}/releases", self.repo)
    }

    /// Release downloads must come from here.
    pub fn download_prefix(&self) -> String {
        format!("https://github.com/{}/releases/download/", self.repo)
    }
}

fn pick<'a>(text: &'a Localized, lang: &str) -> &'a str {
    text.get(lang).or_else(|| text.get("en")).or_else(|| text.values().next()).map_or("", String::as_str)
}

pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

impl Catalog {
    pub fn embedded() -> Self {
        serde_json::from_str(EMBEDDED).expect("the built-in catalog.json is valid")
    }

    fn parse(bytes: &[u8]) -> Option<Self> {
        let c: Self = serde_json::from_slice(bytes).ok()?;
        let valid = !c.apps.is_empty()
            && c.apps.iter().all(|a| {
                !a.id.is_empty()
                    && a.id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
                    && a.repo.split('/').count() == 2
                    && !a.asset_prefix.is_empty()
                    // An outside app always credits and links its creators.
                    && (!a.is_outside() || a.publisher.as_ref().is_some_and(|p| !p.name.trim().is_empty() && p.url.starts_with("https://")))
            });
        valid.then_some(c)
    }

    fn cache_file() -> std::path::PathBuf {
        paths::cache_dir().join("catalog.json")
    }

    /// The last catalog downloaded, or the built-in one.
    pub fn load() -> Self {
        std::fs::read(Self::cache_file()).ok().and_then(|b| Self::parse(&b)).unwrap_or_else(Self::embedded)
    }

    /// Download the current catalog; `None` when offline or the file is not valid.
    pub fn fetch() -> Option<Self> {
        let url = remote_url()?;
        let bytes = crate::net::get_bytes(&url).map_err(|e| log::info!("catalog not refreshed: {e}")).ok()?;
        let catalog = Self::parse(&bytes)?;
        let _ = paths::write_atomic(&Self::cache_file(), &bytes);
        Some(catalog)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_catalog_is_valid() {
        let c = Catalog::parse(EMBEDDED.as_bytes()).expect("valid");
        let dc = c.apps.iter().find(|a| a.id == "draftcraft").expect("DraftCraft is listed");
        assert_eq!(dc.repo, "DraftCraft-DEV/DraftCraft");
        assert_eq!(dc.macos.bundle_id, "dev.skryx.draftcraft");
        assert_eq!(dc.accent_rgb(), Some([0x3e, 0xa6, 0xe8]));
        assert!(!dc.tagline("it").is_empty() && dc.tagline("it") != dc.tagline("en"));
        assert_eq!(dc.tagline("fr"), dc.tagline("en"));
    }

    #[test]
    fn outside_apps_are_kept_apart_and_credit_their_creators() {
        let c = Catalog::parse(EMBEDDED.as_bytes()).expect("valid");
        let dc = c.apps.iter().find(|a| a.id == "draftcraft").expect("DraftCraft is listed");
        assert_eq!(dc.origin, Origin::Dev, "DraftCraft is a D.E.V. app");
        for (id, by, url) in [
            ("tracedraw", "TraceDraw contributors", "https://github.com/tracedraw/tracedraw"),
        ] {
            let a = c.apps.iter().find(|a| a.id == id).unwrap_or_else(|| panic!("{id} is listed"));
            assert!(a.is_outside(), "{id} is an outside app");
            assert_eq!(a.publisher.as_ref().map(|p| p.name.as_str()), Some(by));
            assert_eq!(a.publisher_url(), url);
            assert!(a.license.is_some(), "{id} names its licence");
            assert!(!a.coming_soon, "{id} installs from its own releases");
            assert_eq!(a.download_prefix(), format!("https://github.com/{}/releases/download/", a.repo), "{id} downloads only from its creators' GitHub");
        }
        // D.E.V. apps come first, so the sidebar's two sections are each in one run.
        let first_outside = c.apps.iter().position(AppDef::is_outside).unwrap_or(c.apps.len());
        assert!(c.apps[first_outside..].iter().all(AppDef::is_outside));
    }

    #[test]
    fn an_outside_app_without_credit_is_refused() {
        let base = r#"{"id":"x","name":"X","repo":"a/b","asset_prefix":"x","origin":"outside""#;
        assert!(Catalog::parse(format!("{{\"apps\":[{base}}}]}}").as_bytes()).is_none(), "no publisher");
        let http = format!(r#"{{"apps":[{base},"publisher":{{"name":"A","url":"http://a"}}}}]}}"#);
        assert!(Catalog::parse(http.as_bytes()).is_none(), "the link must be https");
        let ok = format!(r#"{{"apps":[{base},"publisher":{{"name":"A","url":"https://a"}}}}]}}"#);
        assert!(Catalog::parse(ok.as_bytes()).is_some());
    }

    #[test]
    fn a_bad_catalog_is_refused() {
        assert!(Catalog::parse(b"{\"apps\":[]}").is_none());
        assert!(Catalog::parse(b"{\"apps\":[{\"id\":\"../x\",\"name\":\"X\",\"repo\":\"a/b\",\"asset_prefix\":\"x\"}]}").is_none());
        assert!(Catalog::parse(b"not json").is_none());
    }
}
