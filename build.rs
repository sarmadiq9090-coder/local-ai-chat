fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icon.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let version = env!("CARGO_PKG_VERSION");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico")
            .set("ProductName", "Lemon")
            .set("FileDescription", "Lemon - Local AI Chat")
            .set("CompanyName", "Lemon")
            .set("LegalCopyright", "Copyright (c) 2026 Sarmad — MIT License")
            .set("FileVersion", version)
            .set("ProductVersion", version);
        res.compile().expect("failed to embed Windows resources");
    }
}
