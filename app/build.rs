// Embeds icon, VERSIONINFO and the application manifest (Windows only).
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=snowy.manifest");
    println!("cargo:rerun-if-changed=../assets/snowy.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../assets/snowy.ico")
        .set_manifest_file("snowy.manifest")
        .set("ProductName", "Snowy")
        .set("FileDescription", "Snowy \u{2014} desktop weather")
        .set("CompanyName", "Snowy")
        .set("OriginalFilename", "snowy.exe")
        .set("InternalName", "snowy")
        .set("LegalCopyright", "Copyright (c) 2026 berkcantmr. PolyForm Noncommercial 1.0.0");
    // version comes from CARGO_PKG_VERSION automatically
    if let Err(e) = res.compile() {
        panic!("failed to embed Windows resources: {e}");
    }
}
