use std::path::{Path, PathBuf};

/// Keep the WebView2 loader next to the executable for local `cargo run` and
/// tests. Release packaging also copies it explicitly (installer + portable zip).
fn main() {
    println!("cargo:rerun-if-changed=assets/runtime/x64/WebView2Loader.dll");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("x86_64") {
        println!("cargo:warning=the bundled WebView2Loader.dll is x64 only");
        return;
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let Some(profile_dir) = out_dir.ancestors().nth(3) else {
        println!("cargo:warning=could not determine the target profile directory");
        return;
    };

    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("runtime")
        .join("x64")
        .join("WebView2Loader.dll");
    let destination = profile_dir.join("WebView2Loader.dll");
    std::fs::copy(&source, &destination).unwrap_or_else(|err| {
        panic!(
            "failed to stage WebView2Loader.dll from {} to {}: {err}",
            source.display(),
            destination.display()
        )
    });
}
