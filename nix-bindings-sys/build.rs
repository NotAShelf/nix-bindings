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
  println!("cargo:rerun-if-changed=include/wrapper.h");
  println!("cargo:rerun-if-changed=include/nix_api_store_text.h");
  println!("cargo:rerun-if-changed=include/nix_api_expr_shim.h");
  println!("cargo:rerun-if-changed=src/wrappers/add_to_store.cc");
  println!("cargo:rerun-if-changed=src/wrappers/init_path.cc");
  println!("cargo:rerun-if-changed=src/wrappers/eval.cc");

  // docs.rs has no Nix system libraries. Write empty bindings so the crate
  // compiles and return before any pkg-config or cc invocation.
  if env::var("DOCS_RS").is_ok() {
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    std::fs::write(out_path.join("bindings.rs"), "")
      .expect("write stub bindings for docs.rs");
    return;
  }

  // Dynamically get GCC's include path for standard headers (e.g., stdbool.h)
  let gcc_include = Command::new("gcc")
    .arg("-print-file-name=include")
    .output()
    .expect("Failed to get gcc include path")
    .stdout;
  let gcc_include = String::from_utf8_lossy(&gcc_include).trim().to_string();

  let mut builder = bindgen::Builder::default()
    .header("include/wrapper.h")
    .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
    .formatter(bindgen::Formatter::Rustfmt)
    .rustfmt_configuration_file(std::fs::canonicalize(".rustfmt.toml").ok())
    .parse_callbacks(Box::new(ProcessComments))
    .clang_arg(format!("-I{gcc_include}"))
    .clang_arg("-Iinclude");

  // (Cargo feature env var, preprocessor define, optional pkg-config lib name)
  //
  // `shim` has no pkg-config library; it only flips a `-D` so wrapper.h
  // pulls in the shim header for bindgen.
  let libraries: &[(&str, &str, Option<&str>)] = &[
    ("CARGO_FEATURE_STORE", "FEATURE_STORE", Some("nix-store-c")),
    ("CARGO_FEATURE_EXPR", "FEATURE_EXPR", Some("nix-expr-c")),
    ("CARGO_FEATURE_UTIL", "FEATURE_UTIL", Some("nix-util-c")),
    ("CARGO_FEATURE_FLAKE", "FEATURE_FLAKE", Some("nix-flake-c")),
    ("CARGO_FEATURE_MAIN", "FEATURE_MAIN", Some("nix-main-c")),
    ("CARGO_FEATURE_SHIM", "FEATURE_SHIM", None),
  ];

  for (feat_var, define, lib_name) in libraries {
    if env::var(feat_var).is_err() {
      continue;
    }

    if let Some(lib_name) = lib_name {
      let lib = pkg_config::probe_library(lib_name)
        .unwrap_or_else(|_| panic!("Unable to find .pc file for {lib_name}"));

      for include_path in lib.include_paths {
        builder = builder.clang_arg(format!("-I{}", include_path.display()));
      }

      for link_file in lib.link_files {
        println!("cargo:rustc-link-lib={}", link_file.display());
      }
    }

    builder = builder.clang_arg(format!("-D{define}"));
  }

  let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
  let bindings = builder.generate().expect("Unable to generate bindings");
  bindings
    .write_to_file(out_path.join("bindings.rs"))
    .expect("Couldn't write bindings!");

  // Compile the C++ shim. It needs:
  //   - The C++ headers from nix-{store,util} (store-api.hh, etc.).
  //   - The *internal* public C headers from nix-{store,util}-c which give us
  //     the authoritative layouts of nix_c_context, Store, and StorePath
  //     (avoiding the previous hand-copied struct definitions).
  //   - Toolchain system headers (glibc, libstdc++) which on Nix come via
  //     NIX_CFLAGS_COMPILE rather than pkg-config.
  if env::var("CARGO_FEATURE_SHIM").is_ok() {
    assert!(
      env::var("CARGO_FEATURE_STORE").is_ok(),
      "the `shim` feature requires the `store` feature"
    );

    let mut cc_build = cc::Build::new();
    cc_build.cpp(true);
    cc_build.flag("-std=c++23");
    cc_build.flag("-Wno-missing-field-initializers");
    cc_build.flag("-Wno-cpp");
    cc_build.flag("-Wno-interference-size");
    cc_build.flag("-Wno-unused-parameter");
    cc_build.include("include");

    for pc in ["nix-store", "nix-util", "nix-store-c", "nix-util-c"] {
      let lib = pkg_config::probe_library(pc)
        .unwrap_or_else(|_| panic!("Unable to find .pc file for {pc}"));
      for path in &lib.include_paths {
        cc_build.include(path);
      }
    }

    if env::var("CARGO_FEATURE_EXPR").is_ok() {
      // Probe the C API package for its internal headers
      // (nix_api_expr_internal.h, etc.).
      let lib = pkg_config::probe_library("nix-expr-c")
        .unwrap_or_else(|_| panic!("Unable to find .pc file for nix-expr-c"));
      for path in &lib.include_paths {
        cc_build.include(path);
      }

      // eval.cc needs get-drvs.hh from the C++ nix-expr package. Probe for
      // include paths only; cargo_metadata(false) suppresses the transitive
      // link directives (gc, boost, ...) that nix-expr.pc carries and that
      // downstream builds may not expose.
      let expr_lib = pkg_config::Config::new()
        .cargo_metadata(false)
        .probe("nix-expr")
        .unwrap_or_else(|_| panic!("Unable to find .pc file for nix-expr"));
      for path in &expr_lib.include_paths {
        cc_build.include(path);
      }
    }

    // Pull `-isystem` paths from NIX_CFLAGS_COMPILE (toolchain include dirs
    // in a Nix dev shell) and surface them as `-I` to cc-rs. Then blank the
    // var for the child so the cc wrapper doesn't apply them a second time.
    if let Ok(nix_cflags) = env::var("NIX_CFLAGS_COMPILE") {
      let parts: Vec<&str> = nix_cflags.split_whitespace().collect();
      let mut i = 0;
      while i < parts.len() {
        if parts[i] == "-isystem" && i + 1 < parts.len() {
          cc_build.include(parts[i + 1]);
          i += 2;
        } else {
          i += 1;
        }
      }
    }
    cc_build.env("NIX_CFLAGS_COMPILE", "");

    cc_build.file("src/wrappers/add_to_store.cc");

    if env::var("CARGO_FEATURE_EXPR").is_ok() {
      cc_build.file("src/wrappers/init_path.cc");
      cc_build.file("src/wrappers/eval.cc");
      // Both init_path.cc and eval.cc call into C++ libnixexpr (allowPath,
      // getDerivation, autoCallFunction). Force it onto the link line so
      // dependent crates that only use the C API still link correctly.
      println!("cargo:rustc-link-lib=dylib=nixexpr");
    }

    cc_build.compile("nix_shims");
  }
}
