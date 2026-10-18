fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new()
            .file("native/macos.m")
            .flag("-fobjc-arc")
            .flag("-mmacosx-version-min=13.0")
            .compile("dynamiclight_native");
        for framework in [
            "AppKit",
            "AVFoundation",
            "ScreenCaptureKit",
            "CoreMedia",
            "Foundation",
            "ApplicationServices",
        ] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
        println!("cargo:rerun-if-changed=native/macos.m");
    }
    tauri_build::build()
}
