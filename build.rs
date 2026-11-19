//! Gives the Windows executable its icon and version information.

fn main() {
    println!("cargo:rerun-if-changed=assets/icons/app-icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icons/app-icon.ico")
            .set("ProductName", "D.E.V. Launcher")
            .set("FileDescription", "D.E.V. Launcher")
            .set("CompanyName", "DraftCraft Engineering Ventures")
            .set("OriginalFilename", "dev-launcher.exe");
        if let Err(e) = res.compile() {
            println!("cargo:warning=couldn't embed the Windows icon: {e}");
        }
    }
}
