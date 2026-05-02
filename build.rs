fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // On macOS 26 (Tahoe) IOReport ships only as `/usr/lib/libIOReport.dylib`
    // in the dyld shared cache; older macOS had a framework at
    // `/System/Library/PrivateFrameworks/IOReport.framework` instead.
    // Add `/usr/lib` to the linker's search path explicitly so
    // `#[link(name = "IOReport")]` resolves on the dylib path.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-search=native=/usr/lib");
    }
}
