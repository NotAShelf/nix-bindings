use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    // Tell cargo to invalidate the built crate whenever the wrapper changes
    println!("cargo:rerun-if-changed=wrapper.h");

    // We must also link the required libraries for the C API symbols (in *c.so)
    println!("cargo:rustc-link-lib=nixstorec");
    println!("cargo:rustc-link-lib=nixutilc");
    println!("cargo:rustc-link-lib=nixexprc");
    println!("cargo:rustc-link-lib=archive");

    // Detect Nix version using pkg-config
    let nix_version = {
        let output = Command::new("pkg-config")
            .args(&["--modversion", "nix-store"])
            .output()
            .expect("Failed to get nix version");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };

    // Parse version into v{major}_{minor} for versioned module naming
    let version_mod = {
        let mut parts = nix_version.split('.');
        let major = parts.next().unwrap_or("0");
        let minor = parts.next().unwrap_or("0");
        format!("v{}_{}", major, minor)
    };

    // Use pkg-config to find nix-store include and link paths
    // This NEEDS to be included, or otherwise `nix_api_store.h` cannot be found.
    let lib = pkg_config::Config::new()
        .atleast_version("2.0.0")
        .probe("nix-store")
        .expect("Could not find nix-store via pkg-config");

    // Dynamically get GCC's include path for standard headers (e.g., stdbool.h)
    let gcc_include = Command::new("gcc")
        .arg("-print-file-name=include")
        .output()
        .expect("Failed to get gcc include path")
        .stdout;
    let gcc_include = String::from_utf8_lossy(&gcc_include).trim().to_string();

    // The bindgen::Builder is the main entry point to bindgen
    let mut builder = bindgen::Builder::default()
        .header("wrapper.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .formatter(bindgen::Formatter::Prettyplease);

    // Add all pkg-config include paths and GCC's include path to bindgen
    for include_path in &lib.include_paths {
        builder = builder.clang_arg(format!("-I{}", include_path.display()));
    }
    builder = builder.clang_arg(format!("-I{gcc_include}"));

    // Write the bindings to $OUT_DIR/v{major}_{minor}/bindings.rs
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let versioned_dir = out_dir.join(&version_mod);
    fs::create_dir_all(&versioned_dir).expect("Failed to create versioned bindings dir");
    let bindings = builder.generate().expect("Unable to generate bindings");
    bindings
        .write_to_file(versioned_dir.join("bindings.rs"))
        .expect("Couldn't write versioned bindings!");

    // Write a versioned mod.rs to $OUT_DIR/v{major}_{minor}/mod.rs
    // This allows the versioned module to be included from OUT_DIR
    let mod_rs = versioned_dir.join("mod.rs");
    fs::write(
        &mod_rs,
        format!(
            r#"
#[allow(non_upper_case_globals)]
#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
pub const NIX_BINDINGS_VERSION: &str = "{nix_version}";
pub mod bindings {{
    include!(concat!(env!("OUT_DIR"), "/{version_mod}/bindings.rs"));
}}
pub use bindings::*;
"#
        ),
    )
    .expect("Couldn't write versioned mod.rs");

    // Write a top-level versions_mod.rs to $OUT_DIR that exposes the versioned module
    // This is included by src/lib.rs to re-export the correct version
    let versions_mod_rs = out_dir.join("versions_mod.rs");
    fs::write(
        &versions_mod_rs,
        format!("pub mod {ver}; pub use {ver}::*;", ver = version_mod),
    )
    .expect("Couldn't write versions_mod.rs");
}
