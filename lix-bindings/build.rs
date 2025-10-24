fn main() {
  let lib = pkg_config::Config::new()
    .probe("lix-store")
    .expect("Could not find lix-store via pkg-config");

  let mut builder = cc::Build::new();
  builder.file("src/lix_wrapper.cc");
  builder.cpp(true);
  builder.std("c++23");

  for include_path in &lib.include_paths {
    builder.include(include_path);
  }

  for lib in &lib.libs {
    builder.flag(format!("-l{lib}"));
  }

  builder.compile("lix_wrapper");
}
