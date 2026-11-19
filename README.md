<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/dev-logo-dark.svg">
    <img src="assets/brand/dev-logo.svg" height="120" alt="{D.E.V.}: three hexagons, one lime with a hole and two below it, beside the name D.E.V. in braces">
  </picture>
</p>

<h1 align="center">D.E.V. Launcher</h1>

<p align="center">
  <b>Install, update and open the DraftCraft apps, all in one place.</b>
</p>

<p align="center">
  The launcher of DraftCraft Engineering Ventures picks the right build for your computer
  (Windows, macOS or Linux, x86_64 or ARM64, Apple silicon or Intel) and does the whole install
  itself. It keeps your apps up to date, and from the menu bar or the notification area it opens
  any of them in one click.
</p>

<p align="center">
  <a href="https://github.com/DraftCraft-DEV/DEV-Launcher/releases/latest/download/dev-launcher-windows-x64-setup.exe"><img alt="Download for Windows" src="https://img.shields.io/badge/Windows-Download-0078D6?style=for-the-badge&logo=data%3Aimage%2Fsvg%2Bxml%3Bbase64%2CPHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCAyNCAyNCIgZmlsbD0id2hpdGUiPjxwYXRoIGQ9Ik0zIDNoOC41djguNUgzek0xMi41IDNIMjF2OC41aC04LjV6TTMgMTIuNWg4LjVWMjFIM3pNMTIuNSAxMi41SDIxVjIxaC04LjV6Ii8%2BPC9zdmc%2B&logoColor=white"></a>
  <a href="https://github.com/DraftCraft-DEV/DEV-Launcher/releases/latest/download/dev-launcher-macos-universal.dmg"><img alt="Download for macOS" src="https://img.shields.io/badge/macOS-Download-333333?style=for-the-badge&logo=apple&logoColor=white"></a>
  <a href="https://github.com/DraftCraft-DEV/DEV-Launcher/releases/latest/download/dev-launcher-linux-x86_64.AppImage"><img alt="Download for Linux (AppImage)" src="https://img.shields.io/badge/Linux-AppImage-E95420?style=for-the-badge&logo=linux&logoColor=white"></a>
</p>

<p align="center">
  <a href="https://draftcraft.dev"><img height="30" src="https://img.shields.io/badge/Website-draftcraft.dev-14171B?style=for-the-badge&labelColor=B6EE2B" alt="Website: draftcraft.dev"></a>
  <a href="https://discord.gg/YgRK8Eww9x"><img height="30" src="https://img.shields.io/badge/Discord-Join%20the%20server-5865F2?style=for-the-badge&logo=discord&logoColor=white" alt="Join the DraftCraft Discord"></a>
  <a href="https://ko-fi.com/K0O529L823"><img src="https://ko-fi.com/img/githubbutton_sm.svg" alt="Support me on Ko-fi"></a>
</p>

<p align="center">
  <img alt="Status: beta" src="https://img.shields.io/badge/status-beta-6FA800?style=flat-square">
  <img alt="Written in pure Rust" src="https://img.shields.io/badge/pure-Rust-b83a24?style=flat-square&logo=rust&logoColor=white">
  <img alt="Runs on macOS, Windows and Linux" src="https://img.shields.io/badge/runs%20on-macOS%20%C2%B7%20Windows%20%C2%B7%20Linux-555555?style=flat-square">
  <img alt="x86_64 and ARM64" src="https://img.shields.io/badge/arch-x86__64%20%C2%B7%20ARM64-555555?style=flat-square">
  <img alt="License: Apache-2.0" src="https://img.shields.io/badge/license-Apache--2.0-555555?style=flat-square">
</p>

<p align="center">
  <a href="#screenshots">Screenshots</a> ·
  <a href="#downloads">Downloads</a> ·
  <a href="#what-it-does">What it does</a> ·
  <a href="#always-at-hand">Always at hand</a> ·
  <a href="#build-from-source">Build from source</a> ·
  <a href="#found-a-bug">Found a bug?</a> ·
  <a href="#made-by">Made by</a> ·
  <a href="#license">License</a>
</p>

> [!NOTE]
> **This is a beta.** Today the launcher knows one app,
> [DraftCraft™](https://github.com/DraftCraft-DEV/DraftCraft), the open-source 2D drafting app; the other
> apps of DraftCraft Engineering Ventures will show up in it as they are released. It reads the list of
> apps from this repository, so new ones appear without updating the launcher.

<p align="center">
  <img src="docs/screenshots/library-dark.png" width="100%" alt="D.E.V. Launcher's library: DraftCraft's page with its icon, an Update to button, the release notes, the details of the installed copy and the list of versions">
  <br><sub><b>The library</b>: each app with its version, what's new, where it is installed and every earlier release.</sub>
</p>

## Screenshots

<table>
  <tr>
    <td width="50%"><img src="docs/screenshots/library-light.png" alt="The library in the light theme"></td>
    <td width="50%"><img src="docs/screenshots/settings.png" alt="Settings: theme, language, update checks, keeping the launcher in the menu bar and starting it with the computer"></td>
  </tr>
  <tr>
    <td><sub><b>Light or dark</b>: it follows the system, or your choice; English or Italian.</sub></td>
    <td><sub><b>Settings</b>: automatic updates, pre-releases, the menu bar icon and where apps go.</sub></td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/setup.png" alt="The launcher's own install screen on Windows: install for only me or everyone, the install folder and a desktop shortcut"></td>
    <td><img src="docs/screenshots/setup-done.png" alt="The install screen once finished: D.E.V. Launcher is installed, with a toggle to open it now"></td>
  </tr>
  <tr>
    <td colspan="2"><sub><b>Its own installer</b> on Windows and Linux: no stock setup wizard, just the launcher installing itself.</sub></td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/dmg.png" alt="The macOS disk image: drag the launcher onto the Applications folder"></td>
    <td><img src="docs/screenshots/uninstall.png" alt="The uninstall screen, with the option to also remove settings and downloads"></td>
  </tr>
  <tr>
    <td><sub><b>On macOS</b>, open the disk image and drag the launcher into Applications.</sub></td>
    <td><sub><b>Uninstalling</b> leaves the apps you installed with it where they are.</sub></td>
  </tr>
</table>

## Downloads

One click downloads the newest version from the [latest release](https://github.com/DraftCraft-DEV/DEV-Launcher/releases/latest):

| System | Download |
|---|---|
| Windows 10 and 11 | Setup: [64-bit (x64)](https://github.com/DraftCraft-DEV/DEV-Launcher/releases/latest/download/dev-launcher-windows-x64-setup.exe) · [ARM64](https://github.com/DraftCraft-DEV/DEV-Launcher/releases/latest/download/dev-launcher-windows-arm64-setup.exe) — portable zip: [x64](https://github.com/DraftCraft-DEV/DEV-Launcher/releases/latest/download/dev-launcher-windows-x64-portable.zip) · [ARM64](https://github.com/DraftCraft-DEV/DEV-Launcher/releases/latest/download/dev-launcher-windows-arm64-portable.zip) |
| macOS 11 or later | Disk image for Apple silicon and Intel: [dev-launcher-macos-universal.dmg](https://github.com/DraftCraft-DEV/DEV-Launcher/releases/latest/download/dev-launcher-macos-universal.dmg) |
| Linux (any distribution) | AppImage: [x86_64](https://github.com/DraftCraft-DEV/DEV-Launcher/releases/latest/download/dev-launcher-linux-x86_64.AppImage) · [ARM64 (aarch64)](https://github.com/DraftCraft-DEV/DEV-Launcher/releases/latest/download/dev-launcher-linux-aarch64.AppImage) |
| Debian, Ubuntu 22.04 or later | .deb: [x86_64](https://github.com/DraftCraft-DEV/DEV-Launcher/releases/latest/download/dev-launcher-linux-x86_64.deb) · [ARM64 (aarch64)](https://github.com/DraftCraft-DEV/DEV-Launcher/releases/latest/download/dev-launcher-linux-aarch64.deb) |
| Fedora, openSUSE, RHEL | .rpm: [x86_64](https://github.com/DraftCraft-DEV/DEV-Launcher/releases/latest/download/dev-launcher-linux-x86_64.rpm) · [ARM64 (aarch64)](https://github.com/DraftCraft-DEV/DEV-Launcher/releases/latest/download/dev-launcher-linux-aarch64.rpm) |
| Arch Linux, Manjaro, EndeavourOS | pacman package: [x86_64](https://github.com/DraftCraft-DEV/DEV-Launcher/releases/latest/download/dev-launcher-linux-x86_64.pkg.tar.zst) · [ARM64 (aarch64)](https://github.com/DraftCraft-DEV/DEV-Launcher/releases/latest/download/dev-launcher-linux-aarch64.pkg.tar.zst) — or `dev-launcher-bin` from the AUR |

The release page also has a plain `.tar.gz` for each Linux processor and `SHA256SUMS.txt`.

- **Windows** — run the setup (`dev-launcher-windows-x64-setup.exe`). It is the launcher itself, showing
  its own install screen: it installs for your account (or, if you choose, for everyone), adds D.E.V.
  Launcher to the Start menu and, if you want, to the desktop. Uninstall it from **Settings › Apps ›
  Installed apps**. The portable zip runs from any folder instead. Neither is signed yet, so SmartScreen
  may stop it the first time: choose **More info**, then **Run anyway**.
- **macOS** — open the disk image and drag **D.E.V. Launcher** into **Applications**. There is nothing
  else to install; to remove it, move it to the Trash. It isn't notarized yet, so the first time open it
  once, then **System Settings › Privacy & Security › Open Anyway** — or run
  `xattr -dr com.apple.quarantine "/Applications/D.E.V. Launcher.app"`.
- **Linux** — the AppImage installs itself too: make it executable (`chmod +x dev-launcher-linux-x86_64.AppImage`)
  and open it, and it offers to put itself in `~/Applications`, in the applications menu and on your
  `PATH` as `dev-launcher` (or to run without installing). Or install the package for your distribution:
  `sudo apt install ./dev-launcher-linux-x86_64.deb`, `sudo dnf install ./dev-launcher-linux-x86_64.rpm`,
  `sudo pacman -U dev-launcher-linux-x86_64.pkg.tar.zst`, or `yay -S dev-launcher-bin`. On ARM, take
  the `aarch64` file.

## What it does

- **Installs the right build.** It reads each app's GitHub releases and chooses the file for your
  system and processor: the right disk image on a Mac, the x64 or ARM64 setup on Windows, the AppImage
  or your distribution's own package (`.deb`, `.rpm`, Arch) on Linux.
- **Does the install itself.** On macOS it mounts the disk image and copies the app into Applications;
  on Windows it runs the app's setup silently; on Linux it puts the AppImage in place with a menu entry,
  or installs the package (asking for your password once). Every download is checked against the
  release's SHA-256 sums.
- **Keeps apps up to date.** It checks for new versions when it starts and every hour, shows what's new,
  and updates in one click — or on its own, while the app is closed. A running app keeps working and
  picks up the new version when you restart it.
- **Finds what's already there.** Copies you installed yourself are found too, and any earlier version
  can be installed again from the list of releases.

## Always at hand

D.E.V. Launcher starts with your computer and stays out of the way: closing the window leaves a small
icon in the **menu bar** on macOS, in the **notification area** on Windows and in the **system tray** on
Linux.

- Click it to see your installed apps and open any of them at once.
- It keeps checking for updates in the background: the icon gets a dot when one is ready, and the menu
  offers **Update to …** (or installs it for you, if automatic updates are on).
- **Open D.E.V. Launcher** brings the window back; **Quit D.E.V. Launcher** closes the icon until the
  next start.

Both are settings: **Keep running in the menu bar** and **Start with the computer**. On Linux the icon
uses the StatusNotifierItem standard of KDE, Xfce, Cinnamon and most desktops; on GNOME it needs the
AppIndicator extension (Ubuntu has it already).

## Build from source

You need [Rust](https://rustup.rs) 1.90 or later.

```sh
cargo run --release
cargo test
```

A few options help when working on it:

| Option | Does |
|---|---|
| `--screenshot out.png` | saves a picture of the window and quits (with `--page library\|downloads\|settings`, `--theme dark\|light`, `--lang en\|it`) |
| `--app <id>` | opens with that app selected |
| `--tray` | runs only the menu bar / tray icon |
| `--setup`, `--uninstall` | show the install and uninstall screens (Windows, Linux) |
| `--install [--everyone] [--desktop-shortcut]`, `--uninstall --quiet [--remove-data]` | install or uninstall without a window |

The apps it knows are listed in [`catalog.json`](catalog.json). Releases are built by
[`.github/workflows/release.yml`](.github/workflows/release.yml) when a `v*` tag matching `Cargo.toml` is
pushed.

## Found a bug?

Please [open an issue](https://github.com/DraftCraft-DEV/DEV-Launcher/issues/new) with what you did, what you
expected and what happened — a screenshot helps a lot. **Settings › Open log** shows the launcher's log file.
Or come and say it on [Discord](https://discord.gg/YgRK8Eww9x).

Pull requests are welcome too: fork the repository, make your change on a branch, run `cargo test`, and
[open a pull request](https://github.com/DraftCraft-DEV/DEV-Launcher/pulls).

## Made by

D.E.V. Launcher is made by **[DraftCraft Engineering Ventures](https://github.com/DraftCraft-DEV)**:
**[SkryxDev](https://github.com/SkryxDev)** and **[WithShalom Studios](https://github.com/withshalom)**,
the people behind [DraftCraft™](https://github.com/DraftCraft-DEV/DraftCraft). Find out more at
[draftcraft.dev](https://draftcraft.dev), and if you like what we make, you can
[support us on Ko-fi](https://ko-fi.com/K0O529L823).

## License

D.E.V. Launcher is licensed under the [Apache License 2.0](LICENSE). The bundled Inter and Montserrat fonts
keep their own SIL Open Font License ([`assets/fonts`](assets/fonts)). DraftCraft™ and the DraftCraft logo
are trademarks of DraftCraft Engineering Ventures.
