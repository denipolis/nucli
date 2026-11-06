fn main() {
    println!("cargo:rerun-if-changed=assets/icon/nucli.rc");
    #[cfg(windows)]
    {
        let _ = embed_resource::compile("assets/icon/nucli.rc", embed_resource::NONE);
    }
}
