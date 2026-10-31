use std::path::{Path, PathBuf};

/// Keep the WebView2 loader next to the executable for local `cargo run` and
/// tests. Release packaging also copies it explicitly (installer + portable zip).
fn main() {
    println!("cargo:rerun-if-changed=assets/runtime/x64/WebView2Loader.dll");
    println!("cargo:rerun-if-changed=assets/icons/icon.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    #[cfg(windows)]
    embed_windows_resources();

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
    if let Err(err) = std::fs::copy(&source, &destination) {
        if destination.is_file() {
            // A running copy of Inkwell can hold the DLL open. Keep the
            // existing copy rather than failing an otherwise valid build.
            println!(
                "cargo:warning=could not refresh WebView2Loader.dll ({}); keeping the existing copy",
                err
            );
        } else {
            panic!(
                "failed to stage WebView2Loader.dll from {} to {}: {err}",
                source.display(),
                destination.display()
            );
        }
    }
}

/// Embed the application icon and version metadata into the PE so Explorer,
/// shortcuts and the Add/Remove Programs entry can show a proper icon.
#[cfg(windows)]
fn embed_windows_resources() {
    // Prefer the project-local w64devkit binutils when they exist, so a plain
    // `cargo build` works even when MinGW is not on the machine PATH.
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let bin = Path::new(&manifest).join("tools").join("w64devkit").join("bin");
        if bin.join("windres.exe").is_file() {
            let mut paths = vec![bin];
            if let Some(existing) = std::env::var_os("PATH") {
                paths.extend(std::env::split_paths(&existing));
            }
            if let Ok(joined) = std::env::join_paths(paths) {
                std::env::set_var("PATH", joined);
            }
        }
    }

    let icon = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("icons")
        .join("icon.ico");
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon(&icon.display().to_string());
    resource.set("ProductName", "Inkwell");
    resource.set("FileDescription", "Inkwell Markdown reader");
    resource.set("CompanyName", "LeTrollologist");
    resource.set("LegalCopyright", "MIT License");
    resource.set("OriginalFilename", "inkwell.exe");
    resource.set("InternalName", "inkwell");
    resource.set("FileVersion", env!("CARGO_PKG_VERSION"));
    resource.set("ProductVersion", env!("CARGO_PKG_VERSION"));
    resource
        .compile()
        .expect("failed to compile Windows icon/version resources");
}
