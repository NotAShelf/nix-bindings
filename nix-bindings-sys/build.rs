use std::{env, path::PathBuf, process::Command};

use bindgen::callbacks::ParseCallbacks;

#[derive(Debug)]
struct ProcessComments;

impl ParseCallbacks for ProcessComments {
  fn process_comment(&self, comment: &str) -> Option<String> {
    match doxygen_bindgen::transform(comment) {
      Ok(res) => Some(res),
      Err(err) => {
        println!(
          "cargo:warning=Problem processing doxygen comment: {comment}\n{err}"
        );
        None
      },
    }
  }
}

fn main() {
  // Tell cargo to invalidate the built crate whenever the wrapper changes
  println!("cargo:rerun-if-changed=include/wrapper.h");

  // Use pkg-config to find nix-store include and link paths
  // This NEEDS to be included, or otherwise `nix_api_store.h` cannot
  // be found.
  let nix_libraries = [
    "nix-main-c",
    "nix-expr-c",
    "nix-store-c",
    "nix-util-c",
    "nix-flake-c",
  ];

  // Dynamically get GCC's include path for standard headers (e.g., stdbool.h)
  let gcc_include = Command::new("gcc")
    .arg("-print-file-name=include")
    .output()
    .expect("Failed to get gcc include path")
    .stdout;
  let gcc_include = String::from_utf8_lossy(&gcc_include).trim().to_string();

  // The bindgen::Builder is the main entry point to bindgen
  let mut builder = bindgen::Builder::default()
    .header("include/wrapper.h")
    .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
    .formatter(bindgen::Formatter::Rustfmt)
    .rustfmt_configuration_file(std::fs::canonicalize(".rustfmt.toml").ok())
    .parse_callbacks(Box::new(ProcessComments));

  // Add all pkg-config include paths and GCC's include path to bindgen
  for nix_lib in nix_libraries {
    let lib = pkg_config::probe_library(nix_lib)
      .expect(&format!("Unable to find .pc file for {}", nix_lib));
    for include_path in lib.include_paths {
      builder = builder.clang_arg(format!("-I{}", include_path.display()));
    }
    for link_file in lib.link_files {
      println!("cargo:rustc-link-lib={}", link_file.display());
    }
  }
  builder = builder.clang_arg(format!("-I{gcc_include}"));

  // Write the bindings to the $OUT_DIR/bindings.rs file
  let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
  let bindings = builder.generate().expect("Unable to generate bindings");
  bindings
    .write_to_file(out_path.join("bindings.rs"))
    .expect("Couldn't write bindings!");
}
