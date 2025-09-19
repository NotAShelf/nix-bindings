use std::ffi::CStr;

use nix_bindings_sys::nix_version_get;

fn main() {
  unsafe {
    let version_ptr = nix_version_get();
    if version_ptr.is_null() {
      eprintln!("Failed to get Nix version (null pointer)");
      std::process::exit(1);
    }

    let version = CStr::from_ptr(version_ptr).to_string_lossy();
    println!("Nix library version: {version}");
  }
}
