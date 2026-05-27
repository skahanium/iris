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
    tauri_build::build()
}
