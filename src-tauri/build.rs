fn main() {
    let mut attributes = tauri_build::Attributes::new();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut windows = tauri_build::WindowsAttributes::new().static_vc_runtime(true);
        if std::env::var("PROFILE").as_deref() == Ok("release") {
            windows = windows.app_manifest(include_str!("windows/app.manifest"));
        }
        attributes = attributes.windows_attributes(windows);
    }
    tauri_build::try_build(attributes).expect("failed to run Tauri build script");
}
