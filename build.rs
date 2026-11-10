//! Builds the icon and the version information into the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=assets/Go3D.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon_with_id("assets/Go3D.ico", "1")
        .set("CompanyName", "Mykhailo Makarov")
        .set("FileDescription", "3D Go")
        .set("InternalName", "Go3D")
        .set("LegalCopyright", "Copyright (c) 2026 Mykhailo Makarov. MIT License.")
        .set("OriginalFilename", "go3d.exe")
        .set("ProductName", "3D Go");
    // Without the resource compiler the game still builds, only without its icon.
    if let Err(error) = resource.compile() {
        println!("cargo:warning=the icon is left out: {error}");
    }
}
