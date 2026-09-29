fn main() {
    // Manifeste Windows : élévation administrateur obligatoire (lecture SMART) et
    // Common-Controls v6, que Tauri inclut par défaut et qu'un manifeste personnalisé doit reprendre.
    let windows =
        tauri_build::WindowsAttributes::new().app_manifest(include_str!("windows-app.manifest"));
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("échec de tauri-build");
}
