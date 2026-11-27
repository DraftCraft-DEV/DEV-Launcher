//! Finding the copies of an app on this computer, and how each was installed.
//!
//! | System | Found by | Updated by |
//! |---|---|---|
//! | macOS | Spotlight (bundle id), `/Applications`, `~/Applications` | the new `.app` from the DMG swapped in |
//! | Windows, setup | `App Paths\<exe>` under HKCU (this account) or HKLM (everyone) | the release's `-setup.exe`, silently, same scope |
//! | Windows, portable | folders the launcher unpacked | the release's zip, unpacked over it |
//! | Linux, package | `dpkg-query`, `rpm -q`, `pacman -Q` | the release's .deb/.rpm/.pkg.tar.zst through `pkexec` |
//! | Linux, AppImage | the launcher's own, and `~/Applications`, `~/Downloads`, … | the new AppImage renamed over it |

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::catalog::AppDef;
use crate::net::parse_version;
use crate::paths;
use crate::platform::{AssetArch, Kind, Os, Platform};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Method {
    /// macOS `.app` bundle (`path` is the bundle).
    MacApp,
    /// Windows copy installed by the setup/MSI (`path` is its exe).
    WinSetup { for_everyone: bool, product_code: Option<String> },
    /// Windows folder unpacked from the zip (`path` is the folder).
    WinPortable,
    /// Linux AppImage (`path` is the file).
    AppImage,
    /// Linux package of this kind (`path` is the program on `PATH`).
    Package(Kind),
}

impl Method {
    pub fn label(&self) -> String {
        match self {
            Self::MacApp => "macOS app".into(),
            Self::WinSetup { for_everyone: true, .. } => "Windows Installer (all users)".into(),
            Self::WinSetup { for_everyone: false, .. } => "Windows Installer (this user)".into(),
            Self::WinPortable => "Portable".into(),
            Self::AppImage => "AppImage".into(),
            Self::Package(Kind::Deb) => "Debian package".into(),
            Self::Package(Kind::Rpm) => "RPM package".into(),
            Self::Package(Kind::Pacman) => "Arch package".into(),
            Self::Package(k) => k.label().into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Installed {
    pub method: Method,
    pub path: PathBuf,
    pub version: Option<Version>,
    /// macOS: what the binary is built for (an Intel copy on Apple silicon runs under Rosetta).
    pub arch: Option<AssetArch>,
    /// The launcher put it there.
    pub managed: bool,
}

impl Installed {
    /// The program to start.
    pub fn program(&self, app: &AppDef) -> PathBuf {
        match &self.method {
            Method::WinPortable => self.path.join(&app.windows.exe),
            _ => self.path.clone(),
        }
    }

    /// The folder holding it (to show in the file manager).
    pub fn location(&self) -> PathBuf {
        match &self.method {
            Method::WinPortable => self.path.clone(),
            _ => self.path.parent().map_or_else(|| self.path.clone(), Path::to_path_buf),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// What the launcher installed, remembered between runs.

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub app: String,
    pub method: Method,
    pub path: PathBuf,
    pub version: String,
}

fn manifest_file() -> PathBuf {
    paths::config_dir().join("installs.json")
}

static MANIFEST_LOCK: Mutex<()> = Mutex::new(());

pub fn manifest() -> Vec<Record> {
    let _guard = MANIFEST_LOCK.lock();
    std::fs::read(manifest_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Remember (or forget, with `None`) what the launcher installed at `path`.
pub fn remember(app: &str, path: &Path, record: Option<(Method, String)>) {
    let mut all = manifest();
    let _guard = MANIFEST_LOCK.lock();
    all.retain(|r| !(r.app == app && r.path == path));
    if let Some((method, version)) = record {
        all.push(Record { app: app.into(), method, path: path.to_path_buf(), version });
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(&all) {
        let _ = paths::write_atomic(&manifest_file(), &bytes);
    }
}

// ---------------------------------------------------------------------------------------------

/// Every copy of `app` found, best first (the launcher's own, then the newest).
pub fn detect(app: &AppDef, platform: &Platform) -> Vec<Installed> {
    let records: Vec<Record> = manifest().into_iter().filter(|r| r.app == app.id).collect();
    let mut found = match platform.os {
        Os::MacOs => detect_mac(app, &records),
        Os::Windows => detect_windows(app, &records),
        Os::Linux => detect_linux(app, &records),
    };
    found.sort_by(|a, b| b.managed.cmp(&a.managed).then_with(|| b.version.cmp(&a.version)));
    found
}

fn record_for<'a>(records: &'a [Record], path: &Path) -> Option<&'a Record> {
    records.iter().find(|r| r.path == path)
}

fn canonical(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

// ---------------------------------------------------------------------------------------------
// macOS

fn detect_mac(app: &AppDef, records: &[Record]) -> Vec<Installed> {
    if app.macos.bundle_id.is_empty() {
        return Vec::new();
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(out) = Command::new("/usr/bin/mdfind").arg(format!("kMDItemCFBundleIdentifier == '{}'", app.macos.bundle_id)).output() {
        candidates.extend(String::from_utf8_lossy(&out.stdout).lines().filter(|l| !l.is_empty()).map(PathBuf::from));
    }
    candidates.push(PathBuf::from("/Applications").join(&app.macos.app_name));
    candidates.push(paths::user_applications().join(&app.macos.app_name));
    candidates.extend(records.iter().map(|r| r.path.clone()));
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for c in candidates {
        let c = canonical(&c);
        if seen.contains(&c) || !c.is_dir() || is_hidden_copy(&c) {
            continue;
        }
        seen.push(c.clone());
        let Some(info) = mac_bundle_info(&c) else { continue };
        if info.get("CFBundleIdentifier").map(String::as_str) != Some(app.macos.bundle_id.as_str()) {
            continue;
        }
        let version = info.get("CFBundleShortVersionString").and_then(|v| parse_version(v));
        let arch = info.get("CFBundleExecutable").and_then(|exe| mac_binary_arch(&c.join("Contents/MacOS").join(exe)));
        out.push(Installed { method: Method::MacApp, managed: record_for(records, &c).is_some(), path: c, version, arch });
    }
    out
}

/// Copies that aren't installs: in the Trash, on a mounted disk image, in a build folder, or
/// the launcher's own staging copies.
fn is_hidden_copy(p: &Path) -> bool {
    let s = p.to_string_lossy();
    s.contains("/.Trash/")
        || s.starts_with("/Volumes/")
        || s.contains("/target/")
        || s.contains("/Library/Caches/")
        || p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'))
}

/// The bundle's Info.plist values (strings only).
pub fn mac_bundle_info(bundle: &Path) -> Option<HashMap<String, String>> {
    let plist = bundle.join("Contents/Info.plist");
    let out = Command::new("/usr/bin/plutil").args(["-convert", "json", "-o", "-"]).arg(&plist).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    Some(value.as_object()?.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect())
}

/// Which processors the app's executable runs on, read from its Mach-O header. (Not with
/// `/usr/bin/lipo`: on a Mac without the developer tools that is only a stub, which asks to
/// install them every time it runs.)
fn mac_binary_arch(exe: &Path) -> Option<AssetArch> {
    use std::io::Read;
    let mut header = Vec::new();
    std::fs::File::open(exe).ok()?.take(MACHO_HEADER_READ).read_to_end(&mut header).ok()?;
    macho_arch(&header)
}

/// Enough of the file for a universal binary's table of slices.
const MACHO_HEADER_READ: u64 = 4096;
const CPU_TYPE_X86_64: u32 = 0x0100_0007;
const CPU_TYPE_ARM64: u32 = 0x0100_000C;

/// The processors a Mach-O file (thin, or universal with 32- or 64-bit offsets) is built for.
fn macho_arch(header: &[u8]) -> Option<AssetArch> {
    let word = |at: usize, big: bool| -> Option<u32> {
        let b: [u8; 4] = header.get(at..at.checked_add(4)?)?.try_into().ok()?;
        Some(if big { u32::from_be_bytes(b) } else { u32::from_le_bytes(b) })
    };
    let mut cpus = Vec::new();
    match word(0, true)? {
        // Universal binaries keep their header big-endian.
        magic @ (0xCAFE_BABE | 0xCAFE_BABF) => {
            let entry = if magic == 0xCAFE_BABE { 20 } else { 32 };
            let count = word(4, true)? as usize;
            // Java class files start with 0xCAFEBABE too; a real universal binary has a few slices.
            if count == 0 || count > 32 {
                return None;
            }
            for i in 0..count {
                cpus.push(word(8 + i * entry, true)?);
            }
        }
        0xFEED_FACE | 0xFEED_FACF => cpus.push(word(4, true)?),
        0xCEFA_EDFE | 0xCFFA_EDFE => cpus.push(word(4, false)?),
        _ => return None,
    }
    let arm = cpus.contains(&CPU_TYPE_ARM64);
    let intel = cpus.contains(&CPU_TYPE_X86_64);
    match (arm, intel) {
        (true, true) => Some(AssetArch::Universal),
        (true, false) => Some(AssetArch::Arm64),
        (false, true) => Some(AssetArch::X64),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// Windows

fn detect_windows(app: &AppDef, records: &[Record]) -> Vec<Installed> {
    let mut out = Vec::new();
    for (exe, for_everyone) in windows_registered(app) {
        let exe = canonical(&exe);
        if out.iter().any(|i: &Installed| i.path == exe) {
            continue;
        }
        let (version, product_code) = windows_product(app, for_everyone);
        let version = version.or_else(|| record_for(records, &exe).and_then(|r| parse_version(&r.version)));
        out.push(Installed {
            method: Method::WinSetup { for_everyone, product_code },
            managed: record_for(records, &exe).is_some(),
            path: exe,
            version,
            arch: None,
        });
    }
    let mut folders: Vec<PathBuf> = records.iter().filter(|r| r.method == Method::WinPortable).map(|r| r.path.clone()).collect();
    folders.push(paths::windows_user_programs().join(&app.name));
    for dir in folders {
        if !dir.join(&app.windows.exe).is_file() || out.iter().any(|i| i.path.starts_with(&dir)) {
            continue;
        }
        let record = record_for(records, &dir);
        let version = record.and_then(|r| parse_version(&r.version)).or_else(|| program_version(&dir.join(&app.windows.exe)));
        out.push(Installed { method: Method::WinPortable, managed: record.is_some(), path: dir, version, arch: None });
    }
    out
}

/// Programs the setup registered under `App Paths`, with whether for everyone.
#[cfg(windows)]
fn windows_registered(app: &AppDef) -> Vec<(PathBuf, bool)> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY};
    if app.windows.installed_exe.is_empty() {
        return Vec::new();
    }
    let key = format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\{}", app.windows.installed_exe);
    [(HKEY_CURRENT_USER, 0, false), (HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY, true), (HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY, true)]
        .into_iter()
        .filter_map(|(hive, view, everyone)| {
            let k = RegKey::predef(hive).open_subkey_with_flags(&key, KEY_READ | view).ok()?;
            let value: String = k.get_value("").ok()?;
            let path = PathBuf::from(value.trim().trim_matches('"'));
            path.is_file().then_some((path, everyone))
        })
        .collect()
}

#[cfg(not(windows))]
fn windows_registered(_app: &AppDef) -> Vec<(PathBuf, bool)> {
    Vec::new()
}

/// The Windows Installer product's version and product code, from the Uninstall keys.
#[cfg(windows)]
fn windows_product(app: &AppDef, for_everyone: bool) -> (Option<Version>, Option<String>) {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY};
    let roots: Vec<(winreg::HKEY, u32)> =
        if for_everyone { vec![(HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY), (HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY)] } else { vec![(HKEY_CURRENT_USER, 0)] };
    for (hive, view) in roots {
        let Ok(uninstall) =
            RegKey::predef(hive).open_subkey_with_flags(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall", KEY_READ | view)
        else {
            continue;
        };
        for name in uninstall.enum_keys().flatten() {
            let Ok(k) = uninstall.open_subkey_with_flags(&name, KEY_READ | view) else { continue };
            let display: String = k.get_value("DisplayName").unwrap_or_default();
            if display != app.windows.display_name {
                continue;
            }
            let version: String = k.get_value("DisplayVersion").unwrap_or_default();
            let code = (name.starts_with('{') && name.ends_with('}')).then_some(name.clone());
            return (parse_version(&version), code);
        }
    }
    (None, None)
}

#[cfg(not(windows))]
fn windows_product(_app: &AppDef, _for_everyone: bool) -> (Option<Version>, Option<String>) {
    (None, None)
}

// ---------------------------------------------------------------------------------------------
// Linux

fn detect_linux(app: &AppDef, records: &[Record]) -> Vec<Installed> {
    let mut out = Vec::new();
    if let Some((kind, version)) = linux_package(&app.linux.package) {
        let binary = find_on_path(&app.linux.binary).unwrap_or_else(|| PathBuf::from("/usr/bin").join(&app.linux.binary));
        out.push(Installed { method: Method::Package(kind), path: binary, version, arch: None, managed: false });
    }
    let mut images: Vec<PathBuf> = records.iter().filter(|r| r.method == Method::AppImage).map(|r| r.path.clone()).collect();
    let home = dirs::home_dir().unwrap_or_default();
    for dir in ["Applications", "AppImages", "Downloads", "Desktop", ".local/bin", "bin", "Apps"] {
        let Ok(entries) = std::fs::read_dir(home.join(dir)) else { continue };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            if name.ends_with(".appimage") && name.starts_with(&app.asset_prefix.to_ascii_lowercase()) {
                images.push(e.path());
            }
        }
    }
    for image in images {
        let image = canonical(&image);
        if !image.is_file() || out.iter().any(|i: &Installed| i.path == image) {
            continue;
        }
        let record = record_for(records, &image);
        let version = record.and_then(|r| parse_version(&r.version)).or_else(|| program_version(&image));
        out.push(Installed { method: Method::AppImage, managed: record.is_some(), path: image, version, arch: None });
    }
    out
}

/// The installed package and its version, from the first package database that has it.
fn linux_package(name: &str) -> Option<(Kind, Option<Version>)> {
    if name.is_empty() {
        return None;
    }
    if let Some(out) = run_quiet("dpkg-query", &["-W", "-f=${db:Status-Status} ${Version}", name])
        && let Some(v) = out.strip_prefix("installed ") {
            return Some((Kind::Deb, package_version(v)));
        }
    if let Some(out) = run_quiet("pacman", &["-Q", name]) {
        return Some((Kind::Pacman, out.split_whitespace().nth(1).and_then(package_version)));
    }
    if let Some(out) = run_quiet("rpm", &["-q", "--qf", "%{VERSION}", name])
        && !out.contains("not installed") {
            return Some((Kind::Rpm, package_version(&out)));
        }
    None
}

/// `2.2.0-1` → 2.2.0; `1:2.2.0~rc.1-1` → 2.2.0-rc.1.
pub fn package_version(v: &str) -> Option<Version> {
    let v = v.trim();
    let v = v.split_once(':').map_or(v, |(_, rest)| rest);
    let v = v.rsplit_once('-').map_or(v, |(ver, _release)| ver);
    parse_version(&v.replace('~', "-"))
}

fn run_quiet(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn find_on_path(program: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join(program)).find(|c| c.is_file()))
}

// ---------------------------------------------------------------------------------------------

/// `<program> --version` → its version ("draftcraft 2.2.0"), within a few seconds. Remembered per
/// file and modification time, as running an AppImage is slow.
pub fn program_version(program: &Path) -> Option<Version> {
    static CACHE: Mutex<Option<HashMap<(PathBuf, SystemTime), Option<Version>>>> = Mutex::new(None);
    let mtime = std::fs::metadata(program).and_then(|m| m.modified()).ok()?;
    let key = (program.to_path_buf(), mtime);
    if let Some(v) = CACHE.lock().ok()?.get_or_insert_with(HashMap::new).get(&key) {
        return v.clone();
    }
    let v = run_version(program);
    CACHE.lock().ok()?.get_or_insert_with(HashMap::new).insert(key, v.clone());
    v
}

fn run_version(program: &Path) -> Option<Version> {
    let mut cmd = Command::new(program);
    cmd.arg("--version").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < Duration::from_secs(8) => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut text = String::new();
    std::io::Read::read_to_string(&mut child.stdout.take()?, &mut text).ok()?;
    text.split_whitespace().find_map(|w| parse_version(w).filter(|_| w.contains('.')))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_versions() {
        assert_eq!(package_version("2.2.0-1"), Some(Version::new(2, 2, 0)));
        assert_eq!(package_version("1:2.2.0-3"), Some(Version::new(2, 2, 0)));
        assert_eq!(package_version("2.3.0~rc.1-1").map(|v| v.to_string()), Some("2.3.0-rc.1".into()));
    }

    #[test]
    fn hidden_copies() {
        assert!(is_hidden_copy(Path::new("/Users/me/.Trash/DraftCraft.app")));
        assert!(is_hidden_copy(Path::new("/Volumes/DraftCraft/DraftCraft.app")));
        assert!(is_hidden_copy(Path::new("/Applications/.DraftCraft-update.app")));
        assert!(!is_hidden_copy(Path::new("/Applications/DraftCraft.app")));
    }

    /// A universal binary's header: big-endian, one (cputype, subtype, offset, size, align) per slice.
    fn fat(cpus: &[u32]) -> Vec<u8> {
        let mut h = 0xCAFE_BABEu32.to_be_bytes().to_vec();
        h.extend((cpus.len() as u32).to_be_bytes());
        for &cpu in cpus {
            for v in [cpu, 0, 0x4000, 0x1000, 14] {
                h.extend(v.to_be_bytes());
            }
        }
        h
    }

    /// A thin 64-bit executable's header, little-endian as on every Mac.
    fn thin(cpu: u32) -> Vec<u8> {
        let mut h = 0xFEED_FACFu32.to_le_bytes().to_vec();
        h.extend(cpu.to_le_bytes());
        h.extend([0; 24]);
        h
    }

    #[test]
    fn mach_o_architectures() {
        assert_eq!(macho_arch(&fat(&[CPU_TYPE_X86_64, CPU_TYPE_ARM64])), Some(AssetArch::Universal));
        assert_eq!(macho_arch(&fat(&[CPU_TYPE_ARM64])), Some(AssetArch::Arm64));
        assert_eq!(macho_arch(&thin(CPU_TYPE_ARM64)), Some(AssetArch::Arm64));
        assert_eq!(macho_arch(&thin(CPU_TYPE_X86_64)), Some(AssetArch::X64));
        // A universal binary with 64-bit offsets: cputype at the start of each 32-byte entry.
        let mut fat64 = 0xCAFE_BABFu32.to_be_bytes().to_vec();
        fat64.extend(2u32.to_be_bytes());
        for cpu in [CPU_TYPE_ARM64, CPU_TYPE_X86_64] {
            fat64.extend(cpu.to_be_bytes());
            fat64.extend([0; 28]);
        }
        assert_eq!(macho_arch(&fat64), Some(AssetArch::Universal));
    }

    #[test]
    fn what_isnt_a_mac_executable_has_no_architecture() {
        assert_eq!(macho_arch(&[]), None);
        assert_eq!(macho_arch(b"#!/bin/sh\n"), None);
        assert_eq!(macho_arch(&thin(0x0000_0012)), None); // PowerPC
        // A Java class file (0xCAFEBABE, then a version number read as thousands of slices).
        assert_eq!(macho_arch(&[0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x41]), None);
        // A table cut short.
        let mut cut = fat(&[CPU_TYPE_X86_64, CPU_TYPE_ARM64]);
        cut.truncate(20);
        assert_eq!(macho_arch(&cut), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_universal_system_executable_is_read_without_lipo() {
        assert_eq!(mac_binary_arch(Path::new("/bin/ls")), Some(AssetArch::Universal));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_running_systems_apps_have_readable_bundles() {
        let info = mac_bundle_info(Path::new("/System/Applications/Calculator.app")).expect("Calculator's Info.plist");
        assert_eq!(info.get("CFBundleIdentifier").map(String::as_str), Some("com.apple.calculator"));
    }
}
