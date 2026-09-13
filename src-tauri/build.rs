fn main() {
    // Tauri embeds the Windows default icon in the generated context. The
    // Tauri build helper watches the configuration file, but not the icon
    // files it names; without these dependencies `tauri dev` can relaunch a
    // stale executable after an icon refresh.
    for icon in [
        "icons/32x32.png",
        "icons/128x128.png",
        "icons/128x128@2x.png",
        "icons/icon.icns",
        "icons/icon.ico",
    ] {
        println!("cargo:rerun-if-changed={icon}");
    }

    tauri_build::build()
}
