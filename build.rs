fn main() {
    println!("cargo:rerun-if-changed=assets/icon/nucli.rc");
    println!("cargo:rerun-if-changed=assets/icon/nucli.manifest");
    #[cfg(windows)]
    {
        let split = [std::ffi::OsStr::new("SPLIT")];
        let macros = if std::env::var_os("CARGO_FEATURE_SPLIT").is_some() { &split[..] } else { embed_resource::NONE };
        let _ = embed_resource::compile("assets/icon/nucli.rc", macros);
    }
}
