//! Where the launcher runs, and which release file fits it.
//!
//! Release files are named `<prefix>-[<version>-]<os>-<arch><suffix>`, for example
//! `draftcraft-macos-arm64.dmg`, `draftcraft-windows-x64-setup.exe` or
//! `draftcraft-linux-aarch64.AppImage`. Anything else (`draftcraft-cli-macos-arm64.zip`,
//! `SHA256SUMS.txt`) is not an app package and is ignored.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Os {
    MacOs,
    Windows,
    Linux,
}

impl Os {
    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            Self::MacOs => "macOS",
            Self::Windows => "Windows",
            Self::Linux => "Linux",
        }
    }

    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// The processor of the machine (not of this binary: an Intel build under Rosetta still reports
/// Apple silicon, so the native build is chosen).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Arch {
    X64,
    Arm64,
}

impl Arch {
    pub fn label(self, os: Os) -> &'static str {
        match (os, self) {
            (Os::MacOs, Self::Arm64) => "Apple silicon",
            (Os::MacOs, Self::X64) => "Intel",
            (_, Self::X64) => "x64",
            (_, Self::Arm64) => "ARM64",
        }
    }

    /// The architecture this binary was compiled for.
    pub fn compiled() -> Self {
        if cfg!(target_arch = "aarch64") { Self::Arm64 } else { Self::X64 }
    }
}

/// Linux distribution families, by the native package format they use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LinuxFamily {
    Debian,
    Fedora,
    Suse,
    Arch,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Platform {
    pub os: Os,
    pub arch: Arch,
    /// Linux only.
    pub family: Option<LinuxFamily>,
    /// A human name for the system: "macOS 26.1", "Ubuntu 24.04 LTS", "Windows 11".
    pub system_name: String,
}

impl Platform {
    pub fn detect() -> Self {
        let os = Os::current();
        let arch = native_arch(os);
        let (family, system_name) = match os {
            Os::Linux => {
                let release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
                let (family, name) = parse_os_release(&release);
                (Some(family), name)
            }
            Os::MacOs => (None, macos_name()),
            Os::Windows => (None, "Windows".to_string()),
        };
        Self { os, arch, family, system_name }
    }

    pub fn label(&self) -> String {
        format!("{} · {}", self.system_name, self.arch.label(self.os))
    }

    /// How well an asset built for `asset` runs here: lower is better, `None` is "doesn't run".
    pub fn arch_rank(&self, asset: AssetArch) -> Option<u8> {
        match (self.os, self.arch, asset) {
            (Os::MacOs, Arch::Arm64, AssetArch::Arm64) => Some(0),
            (Os::MacOs, _, AssetArch::Universal) => Some(1),
            (Os::MacOs, Arch::Arm64, AssetArch::X64) => Some(5), // Rosetta
            (Os::MacOs, Arch::X64, AssetArch::X64) => Some(0),
            (Os::Windows, Arch::Arm64, AssetArch::Arm64) => Some(0),
            (Os::Windows, Arch::Arm64, AssetArch::X64) => Some(5), // x64 emulation
            (Os::Windows, Arch::X64, AssetArch::X64) => Some(0),
            (Os::Linux, a, b) if AssetArch::from(a) == b => Some(0),
            _ => None,
        }
    }

    /// The native package kind of this Linux distribution.
    pub fn native_package(&self) -> Option<Kind> {
        match self.family? {
            LinuxFamily::Debian => Some(Kind::Deb),
            LinuxFamily::Fedora | LinuxFamily::Suse => Some(Kind::Rpm),
            LinuxFamily::Arch => Some(Kind::Pacman),
            LinuxFamily::Other => None,
        }
    }
}

fn native_arch(os: Os) -> Arch {
    match os {
        Os::MacOs => {
            // `hw.optional.arm64` is 1 on Apple silicon, also for a process running under Rosetta.
            let out = std::process::Command::new("/usr/sbin/sysctl").args(["-n", "hw.optional.arm64"]).output();
            match out {
                Ok(o) if String::from_utf8_lossy(&o.stdout).trim() == "1" => Arch::Arm64,
                Ok(_) => Arch::X64,
                Err(_) => Arch::compiled(),
            }
        }
        Os::Windows => windows_arch().unwrap_or_else(Arch::compiled),
        Os::Linux => Arch::compiled(),
    }
}

#[cfg(windows)]
fn windows_arch() -> Option<Arch> {
    use winreg::RegKey;
    use winreg::enums::HKEY_LOCAL_MACHINE;
    // The system's own value: an emulated x64 process still sees ARM64 here.
    let key = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment").ok()?;
    let value: String = key.get_value("PROCESSOR_ARCHITECTURE").ok()?;
    Some(if value.eq_ignore_ascii_case("ARM64") { Arch::Arm64 } else { Arch::X64 })
}

#[cfg(not(windows))]
fn windows_arch() -> Option<Arch> {
    None
}

fn macos_name() -> String {
    let out = std::process::Command::new("/usr/bin/sw_vers").arg("-productVersion").output();
    match out {
        Ok(o) if o.status.success() => format!("macOS {}", String::from_utf8_lossy(&o.stdout).trim()),
        _ => "macOS".into(),
    }
}

/// The family and pretty name from `/etc/os-release`.
pub fn parse_os_release(text: &str) -> (LinuxFamily, String) {
    let get = |key: &str| {
        text.lines().find_map(|l| l.strip_prefix(key).and_then(|v| v.strip_prefix('='))).map(|v| v.trim().trim_matches('"').to_string())
    };
    let ids = format!("{} {}", get("ID").unwrap_or_default(), get("ID_LIKE").unwrap_or_default()).to_ascii_lowercase();
    let has = |names: &[&str]| ids.split_whitespace().any(|id| names.contains(&id));
    let family = if has(&["debian", "ubuntu", "linuxmint", "pop", "elementary", "raspbian", "kali", "zorin"]) {
        LinuxFamily::Debian
    } else if has(&["fedora", "rhel", "centos", "rocky", "almalinux", "nobara", "ultramarine"]) {
        LinuxFamily::Fedora
    } else if has(&["suse", "opensuse", "opensuse-tumbleweed", "opensuse-leap", "sles"]) {
        LinuxFamily::Suse
    } else if has(&["arch", "manjaro", "endeavouros", "cachyos", "garuda", "artix"]) {
        LinuxFamily::Arch
    } else {
        LinuxFamily::Other
    };
    (family, get("PRETTY_NAME").or_else(|| get("NAME")).unwrap_or_else(|| "Linux".into()))
}

/// The processor an asset is built for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssetArch {
    X64,
    Arm64,
    Universal,
}

impl From<Arch> for AssetArch {
    fn from(a: Arch) -> Self {
        match a {
            Arch::X64 => Self::X64,
            Arch::Arm64 => Self::Arm64,
        }
    }
}

/// Package formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Kind {
    /// macOS disk image holding the `.app`.
    Dmg,
    /// Windows setup program wrapping an MSI (`-setup.exe`).
    WinSetup,
    /// Windows Installer package.
    Msi,
    /// Windows portable zip.
    Zip,
    AppImage,
    Deb,
    Rpm,
    /// Arch Linux package (`.pkg.tar.zst`).
    Pacman,
}

impl Kind {
    /// Suffixes, longest first so `-setup.exe` wins over a bare `.exe`.
    const SUFFIXES: &[(&str, Kind)] = &[
        ("-setup.exe", Kind::WinSetup),
        (".pkg.tar.zst", Kind::Pacman),
        (".AppImage", Kind::AppImage),
        (".appimage", Kind::AppImage),
        (".dmg", Kind::Dmg),
        (".msi", Kind::Msi),
        ("-portable.zip", Kind::Zip),
        (".zip", Kind::Zip),
        (".deb", Kind::Deb),
        (".rpm", Kind::Rpm),
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Dmg => "DMG",
            Self::WinSetup => "Setup",
            Self::Msi => "MSI",
            Self::Zip => "Portable",
            Self::AppImage => "AppImage",
            Self::Deb => ".deb",
            Self::Rpm => ".rpm",
            Self::Pacman => ".pkg.tar.zst",
        }
    }

    pub fn os(self) -> Os {
        match self {
            Self::Dmg => Os::MacOs,
            Self::WinSetup | Self::Msi | Self::Zip => Os::Windows,
            Self::AppImage | Self::Deb | Self::Rpm | Self::Pacman => Os::Linux,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParsedAsset {
    pub os: Os,
    pub arch: AssetArch,
    pub kind: Kind,
}

/// What an asset's file name says, for an app whose files start with `prefix`.
pub fn parse_asset(prefix: &str, name: &str) -> Option<ParsedAsset> {
    let (stem, kind) = Kind::SUFFIXES.iter().find_map(|(suffix, kind)| name.strip_suffix(suffix).map(|s| (s, *kind)))?;
    let rest = stem.strip_prefix(prefix)?.strip_prefix('-')?;
    let mut tokens: Vec<&str> = rest.split('-').collect();
    // An optional version right after the prefix: `draftcraft-2.2.0-macos-arm64.dmg`.
    if tokens.len() == 3 && looks_like_version(tokens[0]) {
        tokens.remove(0);
    }
    let [os, arch] = tokens.as_slice() else { return None };
    let os = match os.to_ascii_lowercase().as_str() {
        "macos" | "mac" | "darwin" | "osx" => Os::MacOs,
        "windows" | "win" | "win64" => Os::Windows,
        "linux" => Os::Linux,
        _ => return None,
    };
    let arch = match arch.to_ascii_lowercase().as_str() {
        "x64" | "x86_64" | "amd64" => AssetArch::X64,
        "arm64" | "aarch64" => AssetArch::Arm64,
        "universal" | "universal2" => AssetArch::Universal,
        _ => return None,
    };
    (kind.os() == os).then_some(ParsedAsset { os, arch, kind })
}

fn looks_like_version(token: &str) -> bool {
    let t = token.strip_prefix('v').unwrap_or(token);
    t.chars().next().is_some_and(|c| c.is_ascii_digit()) && t.contains('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draftcraft_release_names_parse() {
        let p = |n| parse_asset("draftcraft", n);
        assert_eq!(p("draftcraft-macos-arm64.dmg"), Some(ParsedAsset { os: Os::MacOs, arch: AssetArch::Arm64, kind: Kind::Dmg }));
        assert_eq!(p("draftcraft-macos-x86_64.dmg").unwrap().arch, AssetArch::X64);
        assert_eq!(p("draftcraft-windows-x64-setup.exe").unwrap().kind, Kind::WinSetup);
        assert_eq!(p("draftcraft-windows-arm64.zip").unwrap().kind, Kind::Zip);
        assert_eq!(p("draftcraft-linux-aarch64.AppImage").unwrap().arch, AssetArch::Arm64);
        assert_eq!(p("draftcraft-linux-x86_64.deb").unwrap().kind, Kind::Deb);
        assert_eq!(p("draftcraft-2.3.0-linux-x86_64.rpm").unwrap().kind, Kind::Rpm);
        assert_eq!(p("draftcraft-linux-x86_64.pkg.tar.zst").unwrap().kind, Kind::Pacman);
        // Not the app.
        assert_eq!(p("draftcraft-cli-macos-arm64.zip"), None);
        assert_eq!(p("SHA256SUMS.txt"), None);
        assert_eq!(p("draftcraft-macos-arm64.deb"), None);
        assert_eq!(p("otherapp-macos-arm64.dmg"), None);
    }

    #[test]
    fn architectures_fall_back_where_emulation_exists() {
        let mac_arm = Platform { os: Os::MacOs, arch: Arch::Arm64, family: None, system_name: String::new() };
        assert!(mac_arm.arch_rank(AssetArch::Arm64) < mac_arm.arch_rank(AssetArch::Universal));
        assert!(mac_arm.arch_rank(AssetArch::Universal) < mac_arm.arch_rank(AssetArch::X64));
        let mac_intel = Platform { arch: Arch::X64, ..mac_arm.clone() };
        assert_eq!(mac_intel.arch_rank(AssetArch::Arm64), None);
        let linux_arm = Platform { os: Os::Linux, arch: Arch::Arm64, family: Some(LinuxFamily::Debian), system_name: String::new() };
        assert_eq!(linux_arm.arch_rank(AssetArch::X64), None);
        assert_eq!(linux_arm.arch_rank(AssetArch::Arm64), Some(0));
    }

    #[test]
    fn os_release_families() {
        assert_eq!(parse_os_release("ID=ubuntu\nID_LIKE=debian\nPRETTY_NAME=\"Ubuntu 24.04 LTS\"").0, LinuxFamily::Debian);
        assert_eq!(parse_os_release("ID=fedora\nPRETTY_NAME=\"Fedora Linux 43\"").1, "Fedora Linux 43");
        assert_eq!(parse_os_release("ID=\"opensuse-tumbleweed\"\nID_LIKE=\"opensuse suse\"").0, LinuxFamily::Suse);
        assert_eq!(parse_os_release("ID=cachyos\nID_LIKE=arch").0, LinuxFamily::Arch);
        assert_eq!(parse_os_release("ID=nixos").0, LinuxFamily::Other);
    }
}
