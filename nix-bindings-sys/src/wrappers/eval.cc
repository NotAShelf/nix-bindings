// Shims for nix_get_derivation and nix_value_auto_call_function.
//
// These mirror the functions I've proposed in NixOS/nix#15842. The PR has been
// stalled upstream, so we implement them locally using the same strategy:
// access nix::getDerivation and EvalState::autoCallFunction through the C++
// API and wrap them in C linkage with NIXC_CATCH_ERRS error translation.
//
// XXX: check_value_in is NOT yet exported from nix_api_expr_internal.h in
// the installed 2.34.x headers, so we dereference nix_value->value directly
// with explicit null guards instead.
//
// FIXME: hopefully this is temporary. C++ shims are the bane of my existence.

#include <nix/expr/eval.hh>
#include <nix/expr/get-drvs.hh>

#include <nix_api_expr.h>
#include <nix_api_expr_internal.h>
#include <nix_api_store.h>
#include <nix_api_store_internal.h>
#include <nix_api_util.h>
#include <nix_api_util_internal.h>

#include "nix_api_expr_shim.h"

static const nix::Bindings *get_bindings_or_null(nix_value *autoArgs) {
  if (!autoArgs || !autoArgs->value)
    return nullptr;
  auto &v = *autoArgs->value;
  if (v.type() == nix::nAttrs)
    return v.attrs();
  return nullptr;
}

extern "C" {

StorePath *nix_get_derivation(nix_c_context *context, EvalState *state,
                              nix_value *value, bool ignoreAssertionFailures) {
  if (context)
    context->last_err_code = NIX_OK;
  if (!state || !value || !value->value)
    return nullptr;
  try {
    auto &v = *value->value;
    auto maybePkg =
        nix::getDerivation(state->state, v, ignoreAssertionFailures);
    if (!maybePkg)
      return nullptr;
    nix::StorePath sp = maybePkg->requireDrvPath();
    return new StorePath{std::move(sp)};
  }
  NIXC_CATCH_ERRS_NULL
}

nix_err nix_value_auto_call_function(nix_c_context *context, EvalState *state,
                                     nix_value *auto_args, nix_value *fn_val,
                                     nix_value *result) {
  if (context)
    context->last_err_code = NIX_OK;
  if (!state || !fn_val || !fn_val->value || !result || !result->value)
    return nix_set_err_msg(context, NIX_ERR_UNKNOWN, "null argument");
  try {
    auto &fn = *fn_val->value;
    auto &res = *result->value;
    const nix::Bindings *b = get_bindings_or_null(auto_args);
    if (b)
      state->state.autoCallFunction(*b, fn, res);
    else
      state->state.autoCallFunction(nix::Bindings::emptyBindings, fn, res);
  }
  NIXC_CATCH_ERRS
}

} // extern "C"
