// This file is a meta-wrapper for bindgen. Each section is guarded by
// a preprocessor define so that only the headers mapped to enabled Cargo
// features are actually included.

#ifdef FEATURE_STORE
#include <nix_api_store.h>
#endif

#ifdef FEATURE_UTIL
#include <nix_api_util.h>
#include <nix_api_external.h>
#endif

#ifdef FEATURE_EXPR
#include <nix_api_expr.h>
#include <nix_api_value.h>
#endif

#ifdef FEATURE_FLAKE
#include <nix_api_flake.h>
#endif

#ifdef FEATURE_MAIN
#include <nix_api_main.h>
#endif
