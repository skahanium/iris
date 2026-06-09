fn main() {
    let icons = [
        "icons/icon.ico",
        "icons/32x32.png",
        "icons/128x128.png",
        "icons/128x128@2x.png",
        "icons/icon.png",
    ];
    for icon in icons {
        println!("cargo:rerun-if-changed={icon}");
    }
    #[cfg(windows)]
    {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }
    let attributes = tauri_build_attributes();
    tauri_build::try_build(attributes).expect("failed to run tauri build script")
}

#[cfg(windows)]
fn tauri_build_attributes() -> tauri_build::Attributes {
    let windows = tauri_build::WindowsAttributes::new_without_app_manifest()
        .window_icon_path("icons/icon.ico");
    tauri_build::Attributes::new().windows_attributes(windows)
}

#[cfg(not(windows))]
fn tauri_build_attributes() -> tauri_build::Attributes {
    tauri_build::Attributes::new()
}
