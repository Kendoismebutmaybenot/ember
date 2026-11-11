//! Embeds the icon and version information in ember.exe.

fn main() {
    println!("cargo:rerun-if-changed=assets/wisp.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        let nightly = std::env::var_os("CARGO_FEATURE_NIGHTLY").is_some();
        // Wisp loads resource ID 1 for the tray and launcher icons.
        res.set_icon_with_id("assets/wisp.ico", "1");
        res.set(
            "FileDescription",
            &format!("Ember{}", if nightly { " (Nightly)" } else { "" }),
        );
        res.set("ProductName", if nightly { "Ember Nightly" } else { "Ember" });
        res.set("LegalCopyright", "Copyright (c) 2026 Sumir Seth. MIT License.");
        res.compile().expect("failed to embed Windows resources");
    }
}
