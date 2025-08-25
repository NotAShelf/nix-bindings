use std::{env, path::PathBuf, process::Command};

fn main() {
  // Tell cargo to invalidate the built crate whenever the wrapper changes
  println!("cargo:rerun-if-changed=wrapper.h");

  // We must also link the required libraries for the C API symbols (in *c.so)
  println!("cargo:rustc-link-lib=nixstorec");
  println!("cargo:rustc-link-lib=nixutilc");
  println!("cargo:rustc-link-lib=nixexprc");
  println!("cargo:rustc-link-lib=archive");

  // Use pkg-config to find nix-store include and link paths
  // This NEEDS to be included, or otherwise `nix_api_store.h` cannot
  // be found.
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
    .formatter(bindgen::Formatter::Rustfmt)
    .rustfmt_configuration_file(std::fs::canonicalize(".rustfmt.toml").ok());

  // Add all pkg-config include paths and GCC's include path to bindgen
  for include_path in &lib.include_paths {
    builder = builder.clang_arg(format!("-I{}", include_path.display()));
  }
  builder = builder.clang_arg(format!("-I{gcc_include}"));

  // Write the bindings to the $OUT_DIR/bindings.rs file
  let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
  let bindings = builder.generate().expect("Unable to generate bindings");
  bindings
    .write_to_file(out_path.join("bindings.rs"))
    .expect("Couldn't write bindings!");
}
