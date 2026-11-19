# D.E.V. Launcher

Install, update and open the apps of **DraftCraft Engineering Ventures** — today
[DraftCraft](https://github.com/DraftCraft-DEV/DraftCraft), more on the way.

The launcher picks the right build for your computer (Windows, macOS, Linux; x86_64 or ARM64;
Apple silicon or Intel) and does the whole install itself: it mounts the DMG and copies the app
on macOS, runs DraftCraft's setup silently on Windows, swaps the AppImage or installs the
`.deb` / `.rpm` / Arch package on Linux. Updates work the same way DraftCraft updates itself.

## Download

From the [releases](https://github.com/DraftCraft-DEV/DEV-Launcher/releases):

| System | File |
| --- | --- |
| macOS (Apple silicon + Intel) | `dev-launcher-macos-universal.dmg` |
| Windows x64 / ARM64 | `dev-launcher-windows-<arch>-setup.exe` or `-portable.zip` |
| Linux (any distro) | `dev-launcher-linux-<arch>.AppImage` |
| Debian, Ubuntu | `dev-launcher-linux-<arch>.deb` |
| Fedora, openSUSE | `dev-launcher-linux-<arch>.rpm` |
| Arch | `dev-launcher-linux-<arch>.pkg.tar.zst`, or `dev-launcher-bin` from the AUR |

The Windows setup and the AppImage are the launcher itself: on first start they show their own
install screen. Uninstall from Windows' "Installed apps", or with `dev-launcher --uninstall`.

## Build

```sh
cargo run
cargo test
```

`--screenshot out.png [--page library|downloads|settings] [--theme dark|light] [--lang en|it]`
saves a picture of the window; `--setup` / `--uninstall` show the install screens.

Releases are built by `.github/workflows/release.yml` when a `v*` tag matching `Cargo.toml` is
pushed.

## License

Apache-2.0
