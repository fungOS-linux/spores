fn main() {
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("wasm32") {
        return;
    }
    // libghostty-vt is built for wasm32-freestanding. The static archives are
    // checked in so a normal wasm build does not need Zig or the Ghostty tree.
    let vendor = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("vendor");
    println!("cargo:rustc-link-search=native={}", vendor.display());
    println!("cargo:rustc-link-lib=static=soilghostty");
    println!("cargo:rustc-link-lib=static=ghostty-vt");
    println!(
        "cargo:rerun-if-changed={}",
        vendor.join("libsoilghostty.a").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        vendor.join("libghostty-vt.a").display()
    );
}
