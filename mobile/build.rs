fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.ends_with("-ios-sim") {
        let entitlements = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ios/Simulator.entitlements");
        println!("cargo:rerun-if-changed={}", entitlements.display());
        println!("cargo:rustc-link-arg-bins=-Wl,-sectcreate,__TEXT,__entitlements,{}", entitlements.display());
    }
}
