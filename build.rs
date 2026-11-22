fn main() {
    println!("cargo::rustc-check-cfg=cfg(elaris_ui, values(\"linux\", \"macos\", \"other\"))");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_OS");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_LINUX_UI");
    // The window's interface picks its layout from `elaris_ui`. The `linux-ui` feature forces the Linux layout so
    // macOS can preview it.
    let ui = if std::env::var_os("CARGO_FEATURE_LINUX_UI").is_some() {
        "linux"
    } else {
        match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
            Ok("linux") => "linux",
            Ok("macos") => "macos",
            _ => "other",
        }
    };
    println!("cargo::rustc-cfg=elaris_ui=\"{ui}\"");

    if std::env::var_os("CARGO_FEATURE_LADYBIRD_BRIDGE").is_none() {
        return;
    }

    let library_dir = std::env::var("ELARIS_LADYBIRD_LIB_DIR")
        .expect("ELARIS_LADYBIRD_LIB_DIR is required with ladybird-bridge");
    println!("cargo:rustc-link-search=native={library_dir}");
    println!("cargo:rustc-link-lib=dylib=elaris_ladybird");
    // Development builds load the bridge from where CMake built it.
    println!("cargo:rustc-link-arg=-Wl,-rpath,{library_dir}");
    println!("cargo:rerun-if-env-changed=ELARIS_LADYBIRD_LIB_DIR");
}
