#![allow(clippy::expect_used)]
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
    .parse_callbacks(Box::new(ProcessComments))
    .clang_arg(format!("-I{gcc_include}"));

  // For each enabled feature, probe the matching C library and add the
  // corresponding preprocessor define so wrapper.h includes the right headers.
  //
  // (Cargo feature env var, preprocessor define, pkg-config lib name)
  let libraries: &[(&str, &str, &str)] = &[
    ("CARGO_FEATURE_STORE", "FEATURE_STORE", "nix-store-c"),
    ("CARGO_FEATURE_EXPR", "FEATURE_EXPR", "nix-expr-c"),
    ("CARGO_FEATURE_UTIL", "FEATURE_UTIL", "nix-util-c"),
    ("CARGO_FEATURE_FLAKE", "FEATURE_FLAKE", "nix-flake-c"),
    ("CARGO_FEATURE_MAIN", "FEATURE_MAIN", "nix-main-c"),
  ];

  for (feat_var, define, lib_name) in libraries {
    if env::var(feat_var).is_ok() {
      let lib = pkg_config::probe_library(lib_name)
        .unwrap_or_else(|_| panic!("Unable to find .pc file for {lib_name}"));

      for include_path in lib.include_paths {
        builder = builder.clang_arg(format!("-I{}", include_path.display()));
      }

      for link_file in lib.link_files {
        println!("cargo:rustc-link-lib={}", link_file.display());
      }

      builder = builder.clang_arg(format!("-D{define}"));
    }
  }

  // Write the bindings to the $OUT_DIR/bindings.rs file
  let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
  let bindings = builder.generate().expect("Unable to generate bindings");
  bindings
    .write_to_file(out_path.join("bindings.rs"))
    .expect("Couldn't write bindings!");
}
