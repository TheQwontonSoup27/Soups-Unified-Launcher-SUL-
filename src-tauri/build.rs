fn main() {
    let windows_msvc = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if !windows_msvc {
        tauri_build::build();
        return;
    }

    // The window code (tao, wry) calls functions that exist under their plain names only in Common
    // Controls v6, which a program gets through an application manifest. tauri-build links its
    // manifest into the app binary alone, so the unit-test executable started without one and
    // Windows refused to load it (0xc0000139, STATUS_ENTRYPOINT_NOT_FOUND).
    //
    // So tauri-build is told not to add the manifest, and the same manifest (the file next to this
    // script, identical to tauri-build's default) is given to the linker, which embeds it in every
    // executable of this package: the app and the tests.
    let windows = tauri_build::WindowsAttributes::new_without_app_manifest();
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("tauri-build failed");

    let manifest =
        std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo"))
            .join("windows-app-manifest.xml");
    println!("cargo:rerun-if-changed=windows-app-manifest.xml");
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
}
