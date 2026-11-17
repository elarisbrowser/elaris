fn main() {
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
