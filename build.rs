//! Build script.
//!
//! On Windows, embeds the application icon (and basic metadata) into the
//! produced `.exe` using `winres`. On every other platform this is a no-op.

fn main() {
    #[cfg(windows)]
    embed_windows_icon();
}

/// Embeds `assets/icons/cretopass.ico` and version metadata into the `.exe`.
#[cfg(windows)]
fn embed_windows_icon() {
    let mut res = winres::WindowsResource::new();
    res.set_icon("assets/icons/cretopass.ico");
    res.set("ProductName", "cretopass");
    res.set("FileDescription", "Terminal-based secret keeper");
    res.set("LegalCopyright", "MIT");
    if let Err(e) = res.compile() {
        // A missing resource compiler should not break the build; the icon
        // simply will not be embedded.
        println!("cargo:warning=failed to embed Windows resources: {e}");
    }

    // Re-run if the icon changes.
    println!("cargo:rerun-if-changed=assets/icons/cretopass.ico");
}
