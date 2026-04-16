fn main() {
    println!("cargo:rerun-if-env-changed=DEP_LITERT_LIB_DIR");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "macos" {
        return;
    }

    // `litert-sys` declares `links = "LiteRt"` and publishes its cache
    // directory as `DEP_LITERT_LIB_DIR`. Embedding it as an rpath makes dyld
    // find libLiteRt.dylib without requiring DYLD_LIBRARY_PATH for downstream
    // test/example binaries.
    if let Ok(dir) = std::env::var("DEP_LITERT_LIB_DIR") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{dir}");
    }
}
