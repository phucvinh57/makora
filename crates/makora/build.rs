fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=icons/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    winresource::WindowsResource::new()
        .set_icon("icons/icon.ico")
        .set("ProductName", "Makora")
        .set("FileDescription", "Makora")
        .set("OriginalFilename", "makora.exe")
        .compile()
        .expect("embedding the Windows icon and version info");
}
