// Shim exposing EvalState::allowPath to the C API.
//
// In pure evaluation mode Nix wraps the filesystem in an
// AllowListSourceAccessor. Path values returned by primops fail when used
// (e.g. as `src` in a derivation) unless the path was registered with the
// evaluator's allowlist first. Nix's own fetch builtins call allowPath after
// adding a path to the store; this shim exposes that same call so that
// plugin primops can do the same.

#include <nix/store/store-api.hh>

#include <nix_api_expr_internal.h>
#include <nix_api_util.h>
#include <nix_api_util_internal.h>

#include "nix_api_expr_shim.h"

nix_err nix_eval_state_allow_path(nix_c_context *context, EvalState *state,
                                  const char *str) {
  if (context)
    context->last_err_code = NIX_OK;
  if (state == nullptr || str == nullptr)
    return nix_set_err_msg(context, NIX_ERR_UNKNOWN, "null argument");
  try {
    auto storePath = state->state.store->parseStorePath(str);
    state->state.allowPath(storePath);
    return NIX_OK;
  }
  NIXC_CATCH_ERRS
}
