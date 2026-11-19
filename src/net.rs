//! GitHub releases: listing them, checksums, and downloading a file with resume and SHA-256 check.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::paths;

/// The largest file the launcher downloads.
pub const MAX_DOWNLOAD: u64 = 2 * 1024 * 1024 * 1024;

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(format!("DEV-Launcher/{}", env!("CARGO_PKG_VERSION")))
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .timeout_per_read(Some(Duration::from_secs(30)))
        .http_status_as_error(false)
        .build()
        .into()
}

/// A GitHub token from the environment, for higher API limits (optional).
fn token() -> Option<String> {
    ["DEV_LAUNCHER_GITHUB_TOKEN", "GITHUB_TOKEN"].iter().find_map(|k| std::env::var(k).ok().filter(|v| !v.trim().is_empty()))
}

pub fn get_bytes(url: &str) -> Result<Vec<u8>, String> {
    let mut resp = agent().get(url).call().map_err(|e| e.to_string())?;
    if resp.status() != 200 {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.body_mut().with_config().limit(16 * 1024 * 1024).read_to_vec().map_err(|e| e.to_string())
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// Lowercase hex.
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Release {
    pub tag: String,
    pub name: String,
    pub body: String,
    /// `2026-11-18T20:15:15Z`
    pub published_at: String,
    pub prerelease: bool,
    pub html_url: String,
    pub assets: Vec<Asset>,
}

impl Release {
    pub fn version(&self) -> Option<Version> {
        parse_version(&self.tag)
    }

    /// `2.2.0` (the tag without its `v`).
    pub fn version_label(&self) -> String {
        self.version().map_or_else(|| self.tag.trim_start_matches('v').to_string(), |v| v.to_string())
    }

    /// `2026-11-18`
    pub fn date(&self) -> &str {
        self.published_at.get(..10).unwrap_or(&self.published_at)
    }

    fn sums_asset(&self) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name.eq_ignore_ascii_case("SHA256SUMS.txt") || a.name.eq_ignore_ascii_case("SHA256SUMS"))
    }
}

pub fn parse_version(s: &str) -> Option<Version> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    Version::parse(s).ok().or_else(|| {
        // `2.2` → `2.2.0`
        let parts: Vec<&str> = s.split('.').collect();
        match parts.len() {
            1 => Version::parse(&format!("{s}.0.0")).ok(),
            2 => Version::parse(&format!("{s}.0")).ok(),
            _ => None,
        }
    })
}

/// GitHub's JSON for a release.
#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
    /// `sha256:<hex>` (GitHub computes it for every upload).
    #[serde(default)]
    digest: Option<String>,
}

fn convert(r: GhRelease) -> Release {
    Release {
        name: r.name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| r.tag_name.clone()),
        tag: r.tag_name,
        body: r.body.unwrap_or_default(),
        published_at: r.published_at.unwrap_or_default(),
        prerelease: r.prerelease,
        html_url: r.html_url,
        assets: r
            .assets
            .into_iter()
            .map(|a| Asset {
                sha256: a.digest.and_then(|d| d.strip_prefix("sha256:").map(str::to_ascii_lowercase)).filter(|h| is_sha256(h)),
                name: a.name,
                url: a.browser_download_url,
                size: a.size,
            })
            .collect(),
    }
}

pub fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// A release list kept between runs with GitHub's ETag, so an unchanged list costs no API quota.
#[derive(Serialize, Deserialize)]
struct Cached {
    etag: Option<String>,
    body: String,
}

fn cache_path(repo: &str) -> std::path::PathBuf {
    paths::cache_dir().join("releases").join(format!("{}.json", repo.replace('/', "__")))
}

/// The newest releases of `repo` (drafts dropped, newest first).
pub fn releases(repo: &str) -> Result<Vec<Release>, String> {
    let url = format!("https://api.github.com/repos/{repo}/releases?per_page=20");
    let cache_file = cache_path(repo);
    let cached: Option<Cached> = std::fs::read(&cache_file).ok().and_then(|b| serde_json::from_slice(&b).ok());
    let mut req = agent().get(&url).header("Accept", "application/vnd.github+json").header("X-GitHub-Api-Version", "2022-11-28");
    if let Some(etag) = cached.as_ref().and_then(|c| c.etag.as_deref()) {
        req = req.header("If-None-Match", etag);
    }
    if let Some(t) = token() {
        req = req.header("Authorization", &format!("Bearer {t}"));
    }
    let body = match req.call() {
        Ok(mut resp) => match resp.status().as_u16() {
            304 => cached.map(|c| c.body).ok_or("empty cache")?,
            200 => {
                let etag = resp.headers().get("etag").and_then(|v| v.to_str().ok()).map(String::from);
                let body = resp.body_mut().with_config().limit(32 * 1024 * 1024).read_to_string().map_err(|e| e.to_string())?;
                if let Ok(bytes) = serde_json::to_vec(&Cached { etag, body: body.clone() }) {
                    let _ = paths::write_atomic(&cache_file, &bytes);
                }
                body
            }
            403 | 429 => match cached {
                // Rate limited: the last list is better than nothing.
                Some(c) => c.body,
                None => return Err("GitHub rate limit reached, try again later".into()),
            },
            404 => return Err(format!("{repo} has no releases")),
            s => return Err(format!("GitHub answered HTTP {s}")),
        },
        Err(e) => match cached {
            Some(c) => {
                log::info!("offline, using cached releases of {repo}: {e}");
                c.body
            }
            None => return Err(format!("couldn't reach GitHub ({e})")),
        },
    };
    let list: Vec<GhRelease> = serde_json::from_str(&body).map_err(|e| format!("unexpected answer from GitHub ({e})"))?;
    let mut releases: Vec<Release> = list.into_iter().filter(|r| !r.draft).map(convert).collect();
    releases.sort_by(|a, b| b.version().cmp(&a.version()).then_with(|| b.published_at.cmp(&a.published_at)));
    Ok(releases)
}

/// Fill in missing checksums from the release's `SHA256SUMS.txt`.
pub fn add_checksums(release: &mut Release, download_prefix: &str) -> Result<(), String> {
    if release.assets.iter().all(|a| a.sha256.is_some()) {
        return Ok(());
    }
    let Some(sums) = release.sums_asset() else { return Ok(()) };
    if !url_under(&sums.url, download_prefix) {
        return Err("the checksum file isn't one of the app's own downloads".into());
    }
    let text = String::from_utf8(get_bytes(&sums.url)?).map_err(|_| "the checksum file isn't text")?;
    let sums = parse_sha256sums(&text);
    for a in &mut release.assets {
        if a.sha256.is_none() {
            a.sha256 = sums.iter().find(|(name, _)| *name == a.name).map(|(_, h)| h.clone());
        }
    }
    Ok(())
}

/// `<hex>  <name>` lines (also `<hex> *<name>`).
pub fn parse_sha256sums(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| {
            let (hash, name) = l.trim().split_once(char::is_whitespace)?;
            let name = name.trim().trim_start_matches('*');
            let name = name.rsplit('/').next().unwrap_or(name);
            is_sha256(hash).then(|| (name.to_string(), hash.to_ascii_lowercase()))
        })
        .collect()
}

/// Whether `url` is a plain https URL under `prefix` (no `..` climbing out of it).
pub fn url_under(url: &str, prefix: &str) -> bool {
    url.starts_with(prefix) && !url[prefix.len()..].split('/').any(|seg| seg == ".." || seg == "." || seg.is_empty()) && !url.contains(['?', '#', '\\'])
}

/// Progress of a download.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Transfer {
    pub done: u64,
    pub total: u64,
    /// Bytes per second, smoothed.
    pub speed: f64,
}

/// Download `asset` to `dest`, resuming a `.part` file left by an earlier try, and check its
/// size and SHA-256. `stop` cancels (the partial file is kept for next time).
pub fn download(asset: &Asset, dest: &Path, stop: &AtomicBool, progress: &mut dyn FnMut(Transfer)) -> Result<(), String> {
    let expected = asset.sha256.as_deref().ok_or_else(|| format!("the release publishes no checksum for {}", asset.name))?;
    if asset.size == 0 || asset.size > MAX_DOWNLOAD {
        return Err(format!("{} has an unexpected size ({} bytes)", asset.name, asset.size));
    }
    // Already downloaded and intact.
    if dest.is_file() && sha256_file(dest).is_ok_and(|h| h == expected) {
        progress(Transfer { done: asset.size, total: asset.size, speed: 0.0 });
        return Ok(());
    }
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("couldn't create {} ({e})", dir.display()))?;
    }
    let part = dest.with_extension(format!("{}part", dest.extension().map(|e| format!("{}.", e.to_string_lossy())).unwrap_or_default()));
    let mut attempts = 0;
    loop {
        attempts += 1;
        match download_once(asset, &part, stop, progress) {
            Ok(()) => break,
            Err(e) if stop.load(Ordering::Relaxed) => return Err(e),
            Err(e) if attempts < 4 => {
                log::warn!("download of {} interrupted ({e}), retrying", asset.name);
                std::thread::sleep(Duration::from_secs(2 * attempts));
            }
            Err(e) => return Err(e),
        }
    }
    let actual = sha256_file(&part).map_err(|e| format!("couldn't read the download ({e})"))?;
    if actual != expected {
        let _ = std::fs::remove_file(&part);
        return Err(format!("{} is damaged (checksum mismatch); try again", asset.name));
    }
    std::fs::rename(&part, dest).map_err(|e| format!("couldn't move the download in place ({e})"))
}

fn download_once(asset: &Asset, part: &Path, stop: &AtomicBool, progress: &mut dyn FnMut(Transfer)) -> Result<(), String> {
    let mut have = std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);
    if have > asset.size {
        let _ = std::fs::remove_file(part);
        have = 0;
    }
    if have == asset.size {
        return Ok(());
    }
    let mut req = agent().get(&asset.url).header("Accept", "application/octet-stream");
    if have > 0 {
        req = req.header("Range", &format!("bytes={have}-"));
    }
    let mut resp = req.call().map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let append = match status {
        206 => true,
        200 => {
            have = 0;
            false
        }
        416 => {
            let _ = std::fs::remove_file(part);
            return Err("the server refused to resume".into());
        }
        s => return Err(format!("HTTP {s}")),
    };
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(part)
        .map_err(|e| format!("couldn't write {} ({e})", part.display()))?;
    let mut reader = resp.body_mut().with_config().limit(MAX_DOWNLOAD).reader();
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = have;
    let mut last = Instant::now();
    let mut last_done = done;
    let mut speed = 0.0;
    progress(Transfer { done, total: asset.size, speed });
    loop {
        if stop.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| format!("couldn't write the download ({e})"))?;
        done += n as u64;
        if done > asset.size {
            return Err("the download is larger than announced".into());
        }
        let dt = last.elapsed().as_secs_f64();
        if dt >= 0.25 {
            let now = (done - last_done) as f64 / dt;
            speed = if speed == 0.0 { now } else { speed * 0.7 + now * 0.3 };
            last = Instant::now();
            last_done = done;
            progress(Transfer { done, total: asset.size, speed });
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    progress(Transfer { done, total: asset.size, speed });
    if done != asset.size {
        return Err(format!("the connection dropped at {done} of {} bytes", asset.size));
    }
    Ok(())
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_files_parse() {
        let text = format!("{0}  draftcraft-macos-arm64.dmg\n{1} *dist/draftcraft-linux-x86_64.AppImage\nnot a line\n", "a".repeat(64), "B".repeat(64));
        let sums = parse_sha256sums(&text);
        assert_eq!(sums.len(), 2);
        assert_eq!(sums[1], ("draftcraft-linux-x86_64.AppImage".to_string(), "b".repeat(64)));
    }

    #[test]
    fn only_downloads_of_the_app_are_accepted() {
        let p = "https://github.com/DraftCraft-DEV/DraftCraft/releases/download/";
        assert!(url_under("https://github.com/DraftCraft-DEV/DraftCraft/releases/download/v2.2.0/draftcraft-macos-arm64.dmg", p));
        assert!(!url_under("https://github.com/DraftCraft-DEV/DraftCraft/releases/download/../../../evil/x", p));
        assert!(!url_under("https://github.com/someone/else/releases/download/v1/x.dmg", p));
        assert!(!url_under("https://github.com/DraftCraft-DEV/DraftCraft/releases/download/v1/x.dmg?x=1", p));
    }

    #[test]
    fn versions_parse_with_or_without_v() {
        assert_eq!(parse_version("v2.2.0"), Some(Version::new(2, 2, 0)));
        assert_eq!(parse_version("2.2"), Some(Version::new(2, 2, 0)));
        assert!(parse_version("v3.0.0-rc.1").is_some_and(|v| !v.pre.is_empty()));
        assert_eq!(parse_version("nightly"), None);
    }

    #[test]
    fn github_digests_become_checksums() {
        let json = r#"[{"tag_name":"v2.2.0","name":"DraftCraft v2.2.0","draft":false,"prerelease":false,"html_url":"h","published_at":"2026-11-18T20:15:15Z","assets":[{"name":"draftcraft-macos-arm64.dmg","browser_download_url":"u","size":5,"digest":"sha256:5043C537223d684e1d0307e27b1a16815d428e349b54bd324605350b8b0bc602"}]},{"tag_name":"v9","draft":true}]"#;
        let list: Vec<GhRelease> = serde_json::from_str(json).unwrap();
        let r: Vec<Release> = list.into_iter().filter(|r| !r.draft).map(convert).collect();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].assets[0].sha256.as_deref(), Some("5043c537223d684e1d0307e27b1a16815d428e349b54bd324605350b8b0bc602"));
        assert_eq!(r[0].date(), "2026-11-18");
        assert_eq!(r[0].version_label(), "2.2.0");
    }
}

#[cfg(test)]
mod live {
    /// Talks to GitHub: `cargo test -- --ignored live`.
    #[test]
    #[ignore]
    fn live_releases_of_draftcraft() {
        let r = super::releases("DraftCraft-DEV/DraftCraft").unwrap();
        assert!(!r.is_empty());
        eprintln!("{} releases, newest {} with {} files", r.len(), r[0].tag, r[0].assets.len());
    }
}
