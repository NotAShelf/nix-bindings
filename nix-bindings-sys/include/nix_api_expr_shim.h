#ifndef NIX_API_EXPR_SHIM_H
#define NIX_API_EXPR_SHIM_H

#include <nix_api_expr.h>
#include <nix_api_util.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @brief Register a store path in the evaluator's access allowlist.
 *
 * In pure evaluation mode (--pure-eval) the Nix evaluator wraps the
 * filesystem in an AllowListSourceAccessor that rejects any path not
 * explicitly permitted.  Nix's own fetch builtins call allowPath after
 * adding a path to the store so that the resulting path value can be
 * used without triggering the access restriction.
 *
 * Call this before nix_init_path_string (or PrimOpRet::set_path) when
 * your primop has added a path to the store and needs to return it as a
 * Nix path value that is usable in pure evaluation mode.
 *
 * @param[out] context Optional. Stores error information.
 * @param[in]  state   Eval state whose allowlist to update.
 * @param[in]  str     Canonical store path string (e.g. /nix/store/...).
 * @return NIX_OK on success, otherwise a nix_err describing the failure.
 */
nix_err nix_eval_state_allow_path(nix_c_context *context, EvalState *state,
                                  const char *str);

#ifdef __cplusplus
}
#endif

#endif /* NIX_API_EXPR_SHIM_H */
