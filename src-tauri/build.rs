fn main() {
    // RC.EXE cannot handle apostrophes in paths (Jad's Brain).
    // Copy the icon to a temp path without special chars first.
    let icon_src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("icons").join("icon.ico");
    let icon_tmp = std::env::temp_dir().join("jads_brain_icon.ico");

    let icon_path = if icon_src.exists() {
        if std::fs::copy(&icon_src, &icon_tmp).is_ok() {
            icon_tmp
        } else {
            icon_src
        }
    } else {
        icon_src
    };

    tauri_build::try_build(
        tauri_build::Attributes::new().windows_attributes(
            tauri_build::WindowsAttributes::new().window_icon_path(&icon_path),
        ),
    )
    .expect("failed to run tauri-build");
}
