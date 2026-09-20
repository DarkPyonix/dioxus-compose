fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        // The Renderer symbols are supplied by the GraalVM shared library at
        // application load time, not while this cdylib is being built.
        println!("cargo:rustc-link-arg-cdylib=-Wl,-undefined,dynamic_lookup");

        if std::env::var_os("CARGO_FEATURE_NATIVE_RENDERER").is_some() {
            let manifest_dir = std::path::PathBuf::from(
                std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets CARGO_MANIFEST_DIR"),
            );
            let renderer_lib_dir =
                manifest_dir.join("../dioxus-compose-renderer/build/native-image/dist/lib");
            println!("cargo:rerun-if-changed={}", renderer_lib_dir.display());
            println!(
                "cargo:rustc-link-search=native={}",
                renderer_lib_dir.display()
            );
            println!("cargo:rustc-link-lib=dylib=dioxus_compose_renderer");
            println!(
                "cargo:rustc-link-arg=-Wl,-rpath,{}",
                renderer_lib_dir.display()
            );
            println!("cargo:rustc-link-arg=-Wl,-export_dynamic");
        }
    }
}
