//! Windows shell calls made directly, instead of through PowerShell: no console window flashes
//! up during setup, and there is no script for an antivirus or a policy to block.

use std::path::Path;

use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile};
use windows::Win32::System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject};
use windows::Win32::UI::Shell::{IShellLinkW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW, ShellLink};
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
use windows::core::{HSTRING, Interface, PCWSTR};

/// Write the shortcut `lnk` to `target` (with its icon and folder).
pub fn shortcut(lnk: &Path, target: &Path, description: &str) -> Result<(), String> {
    if let Some(dir) = lnk.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // SAFETY: plain COM calls on this thread, balanced by `CoUninitialize`.
    unsafe {
        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let result = (|| -> windows::core::Result<()> {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            link.SetPath(&HSTRING::from(target))?;
            if let Some(dir) = target.parent() {
                link.SetWorkingDirectory(&HSTRING::from(dir))?;
            }
            link.SetIconLocation(&HSTRING::from(target), 0)?;
            link.SetDescription(&HSTRING::from(description))?;
            link.cast::<IPersistFile>()?.Save(&HSTRING::from(lnk), true)
        })();
        if init.is_ok() {
            CoUninitialize();
        }
        result.map_err(|e| format!("couldn't create {} ({})", lnk.display(), e.message()))
    }
}

/// Run `exe` with `args` as administrator (Windows asks for permission) and wait for it.
/// Returns its exit code.
pub fn run_as_admin(exe: &Path, args: &[&str]) -> Result<u32, String> {
    let params = args.iter().map(|a| if a.contains(' ') { format!("\"{a}\"") } else { (*a).to_string() }).collect::<Vec<_>>().join(" ");
    let (verb, file, params) = (HSTRING::from("runas"), HSTRING::from(exe), HSTRING::from(params));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    // SAFETY: `info` and the strings it points to outlive the call; the process handle is closed.
    unsafe {
        if let Err(e) = ShellExecuteExW(&mut info) {
            return Err(if e.code() == ERROR_CANCELLED.to_hresult() { "administrator permission was refused".into() } else { format!("couldn't ask for administrator rights ({})", e.message()) });
        }
        if info.hProcess.is_invalid() {
            return Ok(0);
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 0u32;
        let got = GetExitCodeProcess(info.hProcess, &mut code);
        let _ = CloseHandle(info.hProcess);
        got.map_err(|e| e.message().to_string())?;
        Ok(code)
    }
}
