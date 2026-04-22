#[cfg(feature = "generate-header")]
fn main() {
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out = std::path::PathBuf::from(&crate_dir).join("include").join("reflow_rt.h");

    let config = cbindgen::Config {
        language: cbindgen::Language::C,
        cpp_compat: true,
        include_guard: Some("REFLOW_RT_H".into()),
        autogen_warning: Some(
            "/* GENERATED FILE — edit src/lib.rs and rerun `cargo build --features generate-header`. */"
                .into(),
        ),
        enumeration: cbindgen::EnumConfig {
            // Prefix every enum variant with the enum's C name so `Error` in
            // rfl_stream_frame_kind doesn't collide with `Error` in
            // rfl_message_kind at the translation-unit level.
            prefix_with_name: true,
            ..Default::default()
        },
        ..Default::default()
    };

    cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(config)
        .generate()
        .expect("cbindgen failed")
        .write_to_file(out);
}

#[cfg(not(feature = "generate-header"))]
fn main() {}
