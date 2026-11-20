//! Installing, updating and removing apps, entirely from the launcher: nothing is left for the
//! user to open or click through.
//!
//! * **macOS**: the DMG is mounted out of sight, its `.app` is checked (bundle id, version),
//!   copied beside the destination and swapped in by renaming (fine while the app runs), then the
//!   image is ejected.
//! * **Windows (setup)**: the release's `-setup.exe` runs silently (`/quiet --scope user`, or
//!   `/passive --scope machine` when installed for everyone, which shows Windows' permission
//!   prompt). If the app is running, the setup waits for it to quit (`--after <pid>`) and starts
//!   it again (`--relaunch`), the way the app's own Update Now does.
//! * **Windows (portable)**: the zip is unpacked beside the folder and each file moved in; a file
//!   in use is renamed to `*.old` first (Windows allows renaming a running program).
//! * **Linux AppImage**: copied beside the old one and renamed over it; the launcher adds a menu
//!   entry and icon for the copies it installs.
//! * **Linux packages**: `pkexec` runs apt/dnf/zypper/pacman on the downloaded package, so the
//!   package manager keeps its own record right.
//!
//! Every file is checked against the release's SHA-256 before anything is touched.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::catalog::AppDef;
use crate::detect::{self, Installed, Method};
use crate::net::{self, Asset, Release, Transfer};
use crate::paths;
use crate::platform::{Kind, Os, Platform, parse_asset};
use crate::settings::{LinuxFormat, Settings, WinScope};
use crate::system;

/// The most files and bytes a portable zip may unpack to, and how deep it may nest.
const MAX_ZIP_ENTRIES: usize = 5000;
const MAX_ZIP_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_DEPTH: usize = 12;

/// Where an install or update goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// The `.app` bundle to create or replace.
    MacApp { bundle: PathBuf },
    WinSetup { for_everyone: bool },
    WinPortable { dir: PathBuf },
    AppImage { path: PathBuf },
    Package(Kind),
}

impl Target {
    pub fn kind(&self) -> Kind {
        match self {
            Self::MacApp { .. } => Kind::Dmg,
            Self::WinSetup { .. } => Kind::WinSetup,
            Self::WinPortable { .. } => Kind::Zip,
            Self::AppImage { .. } => Kind::AppImage,
            Self::Package(k) => *k,
        }
    }

    /// A short description for menus: "AppImage · ~/Applications".
    pub fn describe(&self) -> String {
        let home = dirs::home_dir().unwrap_or_default();
        let short = |p: &Path| p.strip_prefix(&home).map_or_else(|_| p.display().to_string(), |r| format!("~/{}", r.display()));
        match self {
            Self::MacApp { bundle } => short(bundle.parent().unwrap_or(bundle)),
            Self::WinSetup { for_everyone: true } => "Setup · all users".into(),
            Self::WinSetup { for_everyone: false } => "Setup · this user".into(),
            Self::WinPortable { dir } => format!("Portable · {}", dir.display()),
            Self::AppImage { path } => format!("AppImage · {}", short(path.parent().unwrap_or(path))),
            Self::Package(Kind::Deb) => "Debian package (.deb) · system".into(),
            Self::Package(Kind::Rpm) => "RPM package · system".into(),
            Self::Package(Kind::Pacman) => "Arch package · system".into(),
            Self::Package(k) => k.label().into(),
        }
    }
}

/// A release file and where it goes.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub target: Target,
    pub asset: Asset,
}

/// The best release file of `kind` for this computer.
pub fn best_asset<'a>(app: &AppDef, platform: &Platform, release: &'a Release, kind: Kind) -> Option<&'a Asset> {
    release
        .assets
        .iter()
        .filter_map(|a| {
            let p = parse_asset(&app.asset_prefix, &a.name)?;
            (p.os == platform.os && p.kind == kind).then_some(())?;
            Some((platform.arch_rank(p.arch)?, a))
        })
        .min_by_key(|(rank, _)| *rank)
        .map(|(_, a)| a)
}

/// The ways to install `release` here, the recommended one first.
pub fn install_plans(app: &AppDef, platform: &Platform, release: &Release, settings: &Settings) -> Vec<Plan> {
    let mut plans = Vec::new();
    let mut add = |kind: Kind, target: Target| {
        if let Some(asset) = best_asset(app, platform, release, kind) {
            plans.push(Plan { target, asset: asset.clone() });
        }
    };
    match platform.os {
        Os::MacOs => {
            let system = PathBuf::from("/Applications");
            let dir = if settings.mac_user_applications || !paths::writable(&system) { paths::user_applications() } else { system };
            add(Kind::Dmg, Target::MacApp { bundle: dir.join(&app.macos.app_name) });
        }
        Os::Windows => {
            add(Kind::WinSetup, Target::WinSetup { for_everyone: settings.windows_scope == WinScope::Machine });
            add(Kind::Zip, Target::WinPortable { dir: paths::windows_user_programs().join(&app.name) });
        }
        Os::Linux => {
            let image = Target::AppImage { path: paths::user_applications().join(appimage_name(app)) };
            let native = platform.native_package().filter(|_| detect::find_on_path("pkexec").is_some());
            let native_first = settings.linux_format != LinuxFormat::AppImage;
            if native_first && let Some(k) = native {
                add(k, Target::Package(k));
            }
            add(Kind::AppImage, image);
            if !native_first && let Some(k) = native {
                add(k, Target::Package(k));
            }
        }
    }
    plans
}

fn appimage_name(app: &AppDef) -> String {
    if app.linux.appimage_name.is_empty() { format!("{}.AppImage", app.name) } else { app.linux.appimage_name.clone() }
}

/// How `installed` updates to `release`.
pub fn update_plan(app: &AppDef, platform: &Platform, release: &Release, installed: &Installed) -> Result<Plan, String> {
    let target = match &installed.method {
        Method::MacApp => Target::MacApp { bundle: installed.path.clone() },
        Method::WinSetup { for_everyone, .. } => Target::WinSetup { for_everyone: *for_everyone },
        Method::WinPortable => Target::WinPortable { dir: installed.path.clone() },
        Method::AppImage => Target::AppImage { path: installed.path.clone() },
        Method::Package(k) => Target::Package(*k),
    };
    let kind = target.kind();
    let asset = best_asset(app, platform, release, kind)
        .ok_or_else(|| format!("{} {} has no {} for this computer", app.name, release.version_label(), kind.label()))?;
    Ok(Plan { target, asset: asset.clone() })
}

/// What an install reports while it works.
#[derive(Clone, Debug, PartialEq)]
pub enum Stage {
    Preparing,
    Downloading(Transfer),
    Installing,
    /// The setup waits for the running app to close.
    WaitingForApp,
    /// Asking for administrator permission (pkexec, UAC).
    Authorizing,
}

pub struct Ctx<'a> {
    pub stop: &'a AtomicBool,
    pub report: &'a mut dyn FnMut(Stage),
}

/// What was done.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub version: String,
    /// The app was running and keeps running the old version until restarted.
    pub restart_needed: bool,
}

/// Download `plan`'s file, check it and install it.
pub fn execute(app: &AppDef, release: &Release, plan: &Plan, existing: Option<&Installed>, ctx: &mut Ctx) -> Result<Outcome, String> {
    (ctx.report)(Stage::Preparing);
    let version = release.version_label();
    let prefix = app.download_prefix();
    let mut release = release.clone();
    net::add_checksums(&mut release, &prefix)?;
    let asset = release.assets.iter().find(|a| a.name == plan.asset.name).cloned().ok_or("the release file disappeared")?;
    if !net::url_under(&asset.url, &prefix) {
        return Err(format!("{} isn't one of {}'s own downloads", asset.name, app.name));
    }
    let dir = paths::downloads_dir().join(&app.id).join(sanitize(&release.tag));
    let file = dir.join(&asset.name);
    net::download(&asset, &file, ctx.stop, &mut |t| (ctx.report)(Stage::Downloading(t)))?;
    (ctx.report)(Stage::Installing);
    let running = existing.map(|i| system::running(app, i)).unwrap_or_default();
    let restart_needed = !running.is_empty();
    let result = match &plan.target {
        Target::MacApp { bundle } => {
            install_mac(&file, bundle, &app.macos.bundle_id, &app.macos.app_name, Some(&version), &running).map(|()| {
                detect::remember(&app.id, &canonical_or(bundle), Some((Method::MacApp, version.clone())));
            })
        }
        Target::WinSetup { for_everyone } => run_windows_setup(app, &file, *for_everyone, existing, &running, ctx),
        Target::WinPortable { dir } => install_portable(&file, dir, &app.windows.exe, app.windows.zip_root.as_deref()).map(|()| {
            windows_shortcut(&app.name, &dir.join(&app.windows.exe));
            detect::remember(&app.id, dir, Some((Method::WinPortable, version.clone())));
        }),
        Target::AppImage { path } => install_appimage(&file, path).map(|()| {
            if existing.is_none_or(|e| e.managed) {
                linux_menu_entry(app, path);
            }
            detect::remember(&app.id, &canonical_or(path), Some((Method::AppImage, version.clone())));
        }),
        Target::Package(kind) => {
            (ctx.report)(Stage::Authorizing);
            install_package(*kind, &file)
        }
    };
    result?;
    // The downloaded file isn't needed any more.
    paths::remove_any(&dir);
    log::info!("{} {version} installed ({:?})", app.name, plan.target);
    // A Windows setup started while the app ran relaunches it itself.
    let restart_needed = restart_needed && !matches!(plan.target, Target::WinSetup { .. });
    Ok(Outcome { version, restart_needed })
}

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' }).collect()
}

fn canonical_or(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

// ---------------------------------------------------------------------------------------------
// macOS

fn staged_name(app_name: &str, what: &str) -> String {
    format!(".{}-{what}.app", app_name.trim_end_matches(".app"))
}

/// Mount `dmg`, copy its app (bundle id `bundle_id`, version `version`) beside `bundle` and swap
/// it in. `running` are the processes of the old copy: its leftover is deleted only when none.
pub fn install_mac(dmg: &Path, bundle: &Path, bundle_id: &str, app_name: &str, version: Option<&str>, running: &[u32]) -> Result<(), String> {
    let parent = bundle.parent().ok_or("the destination isn't in a folder")?;
    std::fs::create_dir_all(parent).map_err(|e| format!("couldn't create {} ({e})", parent.display()))?;
    if !paths::writable(parent) {
        return Err(format!("{} isn't writable by this account; choose ~/Applications in Settings", parent.display()));
    }
    let mount = paths::cache_dir().join(format!("mount-{}", std::process::id()));
    paths::remove_any(&mount);
    std::fs::create_dir_all(&mount).map_err(|e| e.to_string())?;
    let os = |s: &'static str| OsStr::new(s);
    system::run(
        "/usr/bin/hdiutil",
        &[os("attach"), os("-readonly"), os("-nobrowse"), os("-noautoopen"), os("-noverify"), os("-mountpoint"), mount.as_os_str(), dmg.as_os_str()],
    )?;
    let staged = parent.join(staged_name(app_name, "update"));
    let copied = (|| {
        let new = find_bundle(&mount, app_name, bundle_id).ok_or_else(|| format!("the disk image doesn't hold {app_name}"))?;
        let info = detect::mac_bundle_info(&new).ok_or("the disk image's app has no Info.plist")?;
        if info.get("CFBundleIdentifier").map(String::as_str) != Some(bundle_id) {
            return Err("the disk image holds a different app".to_string());
        }
        if let Some(want) = version {
            let found = info.get("CFBundleShortVersionString").cloned().unwrap_or_default();
            if net::parse_version(&found) != net::parse_version(want) {
                return Err(format!("the disk image holds version {found}, not {want}"));
            }
        }
        paths::remove_any(&staged);
        system::run("/usr/bin/ditto", &[new.as_os_str(), staged.as_os_str()]).map(|_| ())
    })();
    let _ = system::run("/usr/bin/hdiutil", &[os("detach"), os("-force"), mount.as_os_str()]);
    paths::remove_any(&mount);
    if let Err(e) = copied {
        paths::remove_any(&staged);
        return Err(e);
    }
    // Downloaded with our own client, the copy carries no quarantine flag; clear one anyway.
    let _ = system::run("/usr/bin/xattr", &[os("-dr"), os("com.apple.quarantine"), staged.as_os_str()]);
    let old = parent.join(staged_name(app_name, "old"));
    swap_in(&staged, bundle, &old)?;
    if running.is_empty() {
        paths::remove_any(&old);
    }
    Ok(())
}

/// The `.app` in a mounted image: by name, else any bundle with the id.
fn find_bundle(mount: &Path, app_name: &str, bundle_id: &str) -> Option<PathBuf> {
    let named = mount.join(app_name);
    if named.is_dir() {
        return Some(named);
    }
    std::fs::read_dir(mount).ok()?.flatten().map(|e| e.path()).find(|p| {
        p.extension().is_some_and(|e| e == "app")
            && detect::mac_bundle_info(p).is_some_and(|i| i.get("CFBundleIdentifier").map(String::as_str) == Some(bundle_id))
    })
}

/// Put `new` at `target`, moving what is there to `old` first, and back if the move fails.
pub fn swap_in(new: &Path, target: &Path, old: &Path) -> Result<(), String> {
    paths::remove_any(old);
    let had_old = target.exists();
    if had_old {
        std::fs::rename(target, old).map_err(|e| format!("couldn't move the old {} aside ({e})", target.display()))?;
    }
    if let Err(e) = std::fs::rename(new, target) {
        if had_old {
            let _ = std::fs::rename(old, target);
        }
        return Err(format!("couldn't put the new {} in place ({e})", target.display()));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Windows

/// The setup's arguments: silent for this account, a progress bar (and Windows' permission
/// prompt) for everyone; when the app runs, wait for it and start it again.
pub fn setup_args(for_everyone: bool, after: Option<(u32, &Path)>) -> Vec<std::ffi::OsString> {
    let mut a: Vec<std::ffi::OsString> = vec![
        if for_everyone { "/passive" } else { "/quiet" }.into(),
        "--scope".into(),
        if for_everyone { "machine" } else { "user" }.into(),
    ];
    if let Some((pid, exe)) = after {
        a.extend(["--after".into(), pid.to_string().into(), "--relaunch".into(), exe.as_os_str().to_owned()]);
    }
    a
}

fn run_windows_setup(
    app: &AppDef,
    setup: &Path,
    for_everyone: bool,
    existing: Option<&Installed>,
    running: &[u32],
    ctx: &mut Ctx,
) -> Result<(), String> {
    let after = running.first().copied().zip(existing.map(|e| e.program(app)));
    if after.is_some() {
        (ctx.report)(Stage::WaitingForApp);
    } else if for_everyone {
        (ctx.report)(Stage::Authorizing);
    }
    let args = setup_args(for_everyone, after.as_ref().map(|(pid, exe)| (*pid, exe.as_path())));
    let mut cmd = std::process::Command::new(setup);
    cmd.args(&args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let status = cmd.status().map_err(|e| format!("couldn't start the setup ({e})"))?;
    match status.code() {
        Some(0 | 3010 | 1641) => Ok(()),
        Some(1602) => Err("the installation was cancelled".into()),
        Some(1925) | Some(1730) => Err("installing for everyone needs an administrator".into()),
        Some(1618) => Err("another installation is in progress; try again in a moment".into()),
        Some(c) => Err(format!("the setup failed (Windows Installer error {c})")),
        None => Err("the setup was stopped".into()),
    }
}

/// Unpack the portable zip beside `dir` and move its files in. `exe` must be in it.
pub fn install_portable(zip: &Path, dir: &Path, exe: &str, root: Option<&str>) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("couldn't create {} ({e})", dir.display()))?;
    let staging = dir.join(".dev-launcher-update");
    paths::remove_any(&staging);
    let done = unzip(zip, &staging, root).and_then(|()| {
        if !staging.join(exe).is_file() {
            return Err(format!("the download doesn't hold {exe}"));
        }
        move_in(&staging, dir, 0)
    });
    paths::remove_any(&staging);
    done
}

/// Unpack `zip` into `dest`, dropping its top `root/` folder. Entries that would land outside
/// `dest`, too many entries or too many bytes are refused.
pub fn unzip(zip: &Path, dest: &Path, root: Option<&str>) -> Result<(), String> {
    let file = std::fs::File::open(zip).map_err(|e| format!("couldn't open the download ({e})"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("the download isn't a zip ({e})"))?;
    if archive.len() > MAX_ZIP_ENTRIES {
        return Err("the download holds too many files".into());
    }
    let mut total = 0u64;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("unreadable zip entry ({e})"))?;
        let Some(name) = entry.enclosed_name() else { return Err("a file in the download would land outside its folder".into()) };
        let mut comps = name.components().peekable();
        if let (Some(root), Some(first)) = (root, comps.peek())
            && first.as_os_str().eq_ignore_ascii_case(root)
        {
            comps.next();
        }
        let rel: PathBuf = comps.collect();
        if rel.as_os_str().is_empty() {
            continue;
        }
        if rel.components().count() > MAX_DEPTH {
            return Err("the download's folders nest too deep".into());
        }
        let out = dest.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| format!("couldn't create {} ({e})", out.display()))?;
            continue;
        }
        total = total.saturating_add(entry.size());
        if total > MAX_ZIP_BYTES {
            return Err("the download unpacks to too much".into());
        }
        if let Some(d) = out.parent() {
            std::fs::create_dir_all(d).map_err(|e| format!("couldn't create {} ({e})", d.display()))?;
        }
        let mut f = std::fs::File::create(&out).map_err(|e| format!("couldn't write {} ({e})", out.display()))?;
        let mut limited = std::io::Read::take(&mut entry, MAX_ZIP_BYTES);
        std::io::copy(&mut limited, &mut f).map_err(|e| format!("couldn't unpack {} ({e})", rel.display()))?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode & 0o777));
        }
    }
    Ok(())
}

/// Move every file of `from` into `to`, renaming each file it replaces to `<name>.old`.
fn move_in(from: &Path, to: &Path, depth: usize) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err("the download's folders nest too deep".into());
    }
    for entry in std::fs::read_dir(from).map_err(|e| format!("couldn't read {} ({e})", from.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            std::fs::create_dir_all(&dst).map_err(|e| format!("couldn't create {} ({e})", dst.display()))?;
            move_in(&src, &dst, depth + 1)?;
            continue;
        }
        if dst.exists() {
            let aside = old_name(&dst).ok_or_else(|| format!("couldn't move the old {} aside", dst.display()))?;
            std::fs::rename(&dst, &aside).map_err(|e| format!("couldn't move the old {} aside ({e})", dst.display()))?;
        }
        std::fs::rename(&src, &dst).map_err(|e| format!("couldn't put the new {} in place ({e})", dst.display()))?;
    }
    Ok(())
}

fn old_name(file: &Path) -> Option<PathBuf> {
    let name = file.file_name()?.to_str()?;
    (1..=20u32).map(|n| file.with_file_name(if n == 1 { format!("{name}.old") } else { format!("{name}.old{n}") })).find(|c| !c.exists() || std::fs::remove_file(c).is_ok())
}

/// Delete the `*.old` files an update left in a portable folder (those still in use stay).
pub fn remove_old_files(dir: &Path, depth: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if depth < MAX_DEPTH {
                remove_old_files(&p, depth + 1);
            }
        } else if is_old_name(&p) {
            let _ = std::fs::remove_file(&p);
        }
    }
}

fn is_old_name(p: &Path) -> bool {
    let Some(name) = p.file_name().and_then(|n| n.to_str()) else { return false };
    let Some(pos) = name.rfind(".old") else { return false };
    pos > 0 && name[pos + 4..].bytes().all(|b| b.is_ascii_digit())
}

/// Windows: a Start menu shortcut to a portable copy (best effort).
fn windows_shortcut(name: &str, exe: &Path) {
    #[cfg(windows)]
    {
        let Some(programs) = dirs::data_dir().map(|d| d.join(r"Microsoft\Windows\Start Menu\Programs")) else { return };
        if let Err(e) = crate::winshell::shortcut(&programs.join(format!("{name}.lnk")), exe, name) {
            log::warn!("no Start menu shortcut: {e}");
        }
    }
    #[cfg(not(windows))]
    let _ = (name, exe);
}

// ---------------------------------------------------------------------------------------------
// Linux

/// Copy the new AppImage beside `path`, make it executable and rename it over.
pub fn install_appimage(file: &Path, path: &Path) -> Result<(), String> {
    let dir = path.parent().ok_or("the AppImage isn't in a folder")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("couldn't create {} ({e})", dir.display()))?;
    let staged = dir.join(format!(".{}.dev-launcher-update", path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()));
    std::fs::copy(file, &staged).map_err(|e| format!("couldn't copy the new AppImage ({e})"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)) {
            let _ = std::fs::remove_file(&staged);
            return Err(format!("couldn't make the new AppImage executable ({e})"));
        }
    }
    std::fs::rename(&staged, path).map_err(|e| {
        let _ = std::fs::remove_file(&staged);
        format!("couldn't put the new AppImage in place ({e})")
    })
}

fn menu_entry_path(app: &AppDef) -> PathBuf {
    paths::linux_desktop_entries().join(format!("dev-launcher-{}.desktop", app.id))
}

fn menu_icon_path(app: &AppDef) -> PathBuf {
    paths::linux_icons().join(format!("dev-launcher-{}.png", app.id))
}

/// A menu entry and icon for an AppImage the launcher installed.
fn linux_menu_entry(app: &AppDef, appimage: &Path) {
    if !cfg!(target_os = "linux") {
        return;
    }
    if let Some(png) = crate::assets::app_icon_png(&app.id) {
        let _ = paths::write_atomic(&menu_icon_path(app), png);
    }
    let exec = appimage.display().to_string().replace('"', "\\\"");
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName={}\nComment={}\nExec=\"{exec}\" %F\nIcon={}\nTerminal=false\nCategories=Graphics;Engineering;\nStartupWMClass={}\nX-DEV-Launcher=true\n",
        app.name,
        app.tagline("en"),
        menu_icon_path(app).display(),
        if app.linux.desktop_id.is_empty() { &app.id } else { &app.linux.desktop_id },
    );
    if let Err(e) = paths::write_atomic(&menu_entry_path(app), entry.as_bytes()) {
        log::warn!("no menu entry for {}: {e}", app.name);
    }
    let _ = std::process::Command::new("update-desktop-database").arg(paths::linux_desktop_entries()).output();
}

/// Install a downloaded package with the system's package manager, through `pkexec`.
pub fn install_package(kind: Kind, file: &Path) -> Result<(), String> {
    let file = file.canonicalize().map_err(|e| e.to_string())?;
    let f = file.as_os_str();
    let os = OsStr::new;
    let (program, args): (&str, Vec<&OsStr>) = match kind {
        Kind::Deb if detect::find_on_path("apt-get").is_some() => {
            ("apt-get", vec![os("install"), os("-y"), os("--allow-downgrades"), os("--reinstall"), f])
        }
        Kind::Deb => ("dpkg", vec![os("-i"), f]),
        Kind::Rpm if detect::find_on_path("dnf").is_some() => ("dnf", vec![os("install"), os("-y"), f]),
        Kind::Rpm if detect::find_on_path("zypper").is_some() => {
            ("zypper", vec![os("--non-interactive"), os("install"), os("--allow-unsigned-rpm"), os("--force"), f])
        }
        Kind::Rpm => ("rpm", vec![os("-U"), os("--force"), f]),
        Kind::Pacman => ("pacman", vec![os("-U"), os("--noconfirm"), f]),
        k => return Err(format!("{} isn't a system package", k.label())),
    };
    as_admin(program, &args)
}

/// Run a package manager as administrator through polkit's `pkexec`.
fn as_admin(program: &str, args: &[&OsStr]) -> Result<(), String> {
    let pkexec = detect::find_on_path("pkexec").ok_or("installing system packages needs pkexec (polkit)")?;
    let tool = detect::find_on_path(program).ok_or_else(|| format!("{program} not found"))?;
    let out = std::process::Command::new(pkexec)
        .arg(tool)
        .args(args)
        .env("DEBIAN_FRONTEND", "noninteractive")
        .output()
        .map_err(|e| format!("couldn't run pkexec ({e})"))?;
    match out.status.code() {
        Some(0) => Ok(()),
        Some(126) => Err("administrator permission was not given".into()),
        Some(127) => Err("not authorized to install system packages".into()),
        _ => {
            let err = String::from_utf8_lossy(&out.stderr);
            let last = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
            Err(format!("{program} failed: {last}"))
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Removing

pub fn uninstall(app: &AppDef, installed: &Installed) -> Result<(), String> {
    if !system::running(app, installed).is_empty() {
        return Err(format!("close {} first", app.name));
    }
    match &installed.method {
        Method::MacApp => move_to_trash(&installed.path)?,
        Method::WinSetup { for_everyone, product_code } => {
            let code = product_code.as_deref().ok_or("the installer's product code wasn't found; use Settings › Apps")?;
            let ui = if *for_everyone { "/passive" } else { "/qn" };
            let msiexec = std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from("msiexec.exe"), |r| Path::new(&r).join("System32").join("msiexec.exe"));
            system::run(&msiexec.to_string_lossy(), &[OsStr::new("/x"), OsStr::new(code), OsStr::new(ui), OsStr::new("/norestart")])?;
        }
        Method::WinPortable => {
            std::fs::remove_dir_all(&installed.path).map_err(|e| format!("couldn't remove {} ({e})", installed.path.display()))?;
            if let Some(d) = dirs::data_dir() {
                paths::remove_any(&d.join(format!(r"Microsoft\Windows\Start Menu\Programs\{}.lnk", app.name)));
            }
        }
        Method::AppImage => {
            std::fs::remove_file(&installed.path).map_err(|e| format!("couldn't remove {} ({e})", installed.path.display()))?;
            if installed.managed {
                paths::remove_any(&menu_entry_path(app));
                paths::remove_any(&menu_icon_path(app));
            }
        }
        Method::Package(kind) => {
            let pkg = OsStr::new(&app.linux.package);
            let os = OsStr::new;
            match kind {
                Kind::Deb => as_admin("apt-get", &[os("remove"), os("-y"), pkg])?,
                Kind::Rpm if detect::find_on_path("dnf").is_some() => as_admin("dnf", &[os("remove"), os("-y"), pkg])?,
                Kind::Rpm if detect::find_on_path("zypper").is_some() => as_admin("zypper", &[os("--non-interactive"), os("remove"), pkg])?,
                Kind::Rpm => as_admin("rpm", &[os("-e"), pkg])?,
                Kind::Pacman => as_admin("pacman", &[os("-R"), os("--noconfirm"), pkg])?,
                _ => return Err("unknown package kind".into()),
            }
        }
    }
    detect::remember(&app.id, &installed.path, None);
    Ok(())
}

/// macOS: move to the account's Trash (recoverable), under a free name.
fn move_to_trash(path: &Path) -> Result<(), String> {
    let trash = dirs::home_dir().ok_or("no home folder")?.join(".Trash");
    let name = path.file_name().ok_or("bad path")?.to_string_lossy().to_string();
    let mut dest = trash.join(&name);
    let mut n = 2;
    while dest.exists() {
        dest = trash.join(format!("{} {n}.app", name.trim_end_matches(".app")));
        n += 1;
    }
    std::fs::rename(path, &dest).or_else(|_| {
        // Another volume: let Finder do it.
        let script = format!("tell application \"Finder\" to delete POSIX file \"{}\"", path.display().to_string().replace('"', ""));
        system::run("/usr/bin/osascript", &[OsStr::new("-e"), OsStr::new(&script)]).map(|_| ())
    })
}

/// What earlier updates left beside the copies of `app` (old bundles, `*.old` files, staging).
pub fn clean_leftovers(app: &AppDef, installs: &[Installed]) {
    for i in installs {
        if !system::running(app, i).is_empty() {
            continue;
        }
        match &i.method {
            Method::MacApp => {
                if let Some(parent) = i.path.parent() {
                    paths::remove_any(&parent.join(staged_name(&app.macos.app_name, "old")));
                    paths::remove_any(&parent.join(staged_name(&app.macos.app_name, "update")));
                }
            }
            Method::WinPortable => {
                paths::remove_any(&i.path.join(".dev-launcher-update"));
                remove_old_files(&i.path, 0);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{Arch, LinuxFamily};

    fn draftcraft() -> AppDef {
        crate::catalog::Catalog::embedded().apps.into_iter().find(|a| a.id == "draftcraft").unwrap()
    }

    fn release_220() -> Release {
        let names = [
            "draftcraft-cli-macos-arm64.zip",
            "draftcraft-linux-aarch64.AppImage",
            "draftcraft-linux-aarch64.deb",
            "draftcraft-linux-x86_64.AppImage",
            "draftcraft-linux-x86_64.deb",
            "draftcraft-macos-arm64.dmg",
            "draftcraft-macos-x86_64.dmg",
            "draftcraft-windows-arm64-setup.exe",
            "draftcraft-windows-arm64.zip",
            "draftcraft-windows-x64-setup.exe",
            "draftcraft-windows-x64.zip",
            "SHA256SUMS.txt",
        ];
        Release {
            tag: "v2.2.0".into(),
            assets: names
                .iter()
                .map(|n| Asset {
                    name: (*n).into(),
                    url: format!("https://github.com/DraftCraft-DEV/DraftCraft/releases/download/v2.2.0/{n}"),
                    size: 10,
                    sha256: Some("a".repeat(64)),
                })
                .collect(),
            ..Release::default()
        }
    }

    fn platform(os: Os, arch: Arch, family: Option<LinuxFamily>) -> Platform {
        Platform { os, arch, family, system_name: String::new() }
    }

    #[test]
    fn each_computer_gets_its_own_file() {
        let app = draftcraft();
        let r = release_220();
        for (os, arch, kind, want) in [
            (Os::MacOs, Arch::Arm64, Kind::Dmg, "draftcraft-macos-arm64.dmg"),
            (Os::MacOs, Arch::X64, Kind::Dmg, "draftcraft-macos-x86_64.dmg"),
            (Os::Windows, Arch::X64, Kind::WinSetup, "draftcraft-windows-x64-setup.exe"),
            (Os::Windows, Arch::Arm64, Kind::WinSetup, "draftcraft-windows-arm64-setup.exe"),
            (Os::Windows, Arch::Arm64, Kind::Zip, "draftcraft-windows-arm64.zip"),
            (Os::Linux, Arch::X64, Kind::AppImage, "draftcraft-linux-x86_64.AppImage"),
            (Os::Linux, Arch::Arm64, Kind::Deb, "draftcraft-linux-aarch64.deb"),
        ] {
            let p = platform(os, arch, Some(LinuxFamily::Debian));
            assert_eq!(best_asset(&app, &p, &r, kind).map(|a| a.name.as_str()), Some(want), "{os:?} {arch:?} {kind:?}");
        }
        // No .rpm in this release.
        assert!(best_asset(&app, &platform(Os::Linux, Arch::X64, Some(LinuxFamily::Fedora)), &r, Kind::Rpm).is_none());
    }

    #[test]
    fn windows_installs_through_the_setup_first() {
        let app = draftcraft();
        let plans = install_plans(&app, &platform(Os::Windows, Arch::X64, None), &release_220(), &Settings::default());
        assert_eq!(plans[0].target, Target::WinSetup { for_everyone: false });
        assert_eq!(plans[0].asset.name, "draftcraft-windows-x64-setup.exe");
        assert!(matches!(plans[1].target, Target::WinPortable { .. }));
    }

    #[test]
    fn an_installed_copy_updates_the_way_it_was_installed() {
        let app = draftcraft();
        let r = release_220();
        let p = platform(Os::Windows, Arch::Arm64, None);
        let setup = Installed {
            method: Method::WinSetup { for_everyone: true, product_code: None },
            path: "C:/Program Files/DraftCraft/DraftCraft/draftcraft.exe".into(),
            version: None,
            arch: None,
            managed: false,
        };
        let plan = update_plan(&app, &p, &r, &setup).unwrap();
        assert_eq!(plan.target, Target::WinSetup { for_everyone: true });
        assert_eq!(plan.asset.name, "draftcraft-windows-arm64-setup.exe");
        let deb = Installed { method: Method::Package(Kind::Deb), path: "/usr/bin/draftcraft".into(), version: None, arch: None, managed: false };
        let plan = update_plan(&app, &platform(Os::Linux, Arch::X64, Some(LinuxFamily::Debian)), &r, &deb).unwrap();
        assert_eq!(plan.asset.name, "draftcraft-linux-x86_64.deb");
        let rpm = Installed { method: Method::Package(Kind::Rpm), ..deb };
        assert!(update_plan(&app, &platform(Os::Linux, Arch::X64, Some(LinuxFamily::Fedora)), &r, &rpm).is_err());
    }

    #[test]
    fn setup_arguments() {
        let s = |a: Vec<std::ffi::OsString>| a.into_iter().map(|x| x.to_string_lossy().into_owned()).collect::<Vec<_>>();
        assert_eq!(s(setup_args(false, None)), ["/quiet", "--scope", "user"]);
        assert_eq!(
            s(setup_args(true, Some((42, Path::new("C:/P/draftcraft.exe"))))),
            ["/passive", "--scope", "machine", "--after", "42", "--relaunch", "C:/P/draftcraft.exe"]
        );
    }

    fn tempdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dev-launcher-test-{tag}-{}", std::process::id()));
        paths::remove_any(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_portable_update_keeps_the_old_files_aside_and_settings() {
        let dir = tempdir("portable");
        let folder = dir.join("DraftCraft");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("DraftCraft.exe"), "old").unwrap();
        std::fs::write(folder.join("mine.txt"), "mine").unwrap();
        let zip_path = dir.join("u.zip");
        {
            let mut w = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
            for (n, b) in [("DraftCraft/DraftCraft.exe", "new"), ("DraftCraft/docs/NOTICE", "n")] {
                w.start_file(n, zip::write::SimpleFileOptions::default()).unwrap();
                std::io::Write::write_all(&mut w, b.as_bytes()).unwrap();
            }
            w.finish().unwrap();
        }
        install_portable(&zip_path, &folder, "DraftCraft.exe", Some("DraftCraft")).unwrap();
        assert_eq!(std::fs::read_to_string(folder.join("DraftCraft.exe")).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(folder.join("DraftCraft.exe.old")).unwrap(), "old");
        assert_eq!(std::fs::read_to_string(folder.join("docs/NOTICE")).unwrap(), "n");
        assert_eq!(std::fs::read_to_string(folder.join("mine.txt")).unwrap(), "mine");
        remove_old_files(&folder, 0);
        assert!(!folder.join("DraftCraft.exe.old").exists());
        paths::remove_any(&dir);
    }

    #[test]
    fn a_hostile_zip_changes_nothing() {
        let dir = tempdir("evil");
        let folder = dir.join("App");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("App.exe"), "old").unwrap();
        let zip_path = dir.join("e.zip");
        {
            let mut w = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
            w.start_file("../../outside.txt", zip::write::SimpleFileOptions::default()).unwrap();
            w.finish().unwrap();
        }
        assert!(install_portable(&zip_path, &folder, "App.exe", None).is_err());
        assert!(!dir.join("outside.txt").exists());
        assert_eq!(std::fs::read_to_string(folder.join("App.exe")).unwrap(), "old");
        paths::remove_any(&dir);
    }

    #[test]
    fn swap_in_restores_the_old_copy_on_failure() {
        let dir = tempdir("swap");
        let (new, target, old) = (dir.join("new.app"), dir.join("X.app"), dir.join(".X-old.app"));
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join("v"), "2").unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("v"), "1").unwrap();
        swap_in(&new, &target, &old).unwrap();
        assert_eq!(std::fs::read_to_string(target.join("v")).unwrap(), "2");
        assert!(swap_in(&dir.join("missing.app"), &target, &old).is_err());
        assert_eq!(std::fs::read_to_string(target.join("v")).unwrap(), "2");
        paths::remove_any(&dir);
    }

    #[test]
    fn an_appimage_is_replaced_in_place() {
        let dir = tempdir("appimage");
        let (app, new) = (dir.join("DraftCraft.AppImage"), dir.join("dl"));
        std::fs::write(&app, "old").unwrap();
        std::fs::write(&new, "new").unwrap();
        install_appimage(&new, &app).unwrap();
        assert_eq!(std::fs::read_to_string(&app).unwrap(), "new");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        paths::remove_any(&dir);
    }
}
