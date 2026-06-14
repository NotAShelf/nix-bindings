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

/**
 * @brief Determine whether a Nix value is a derivation and return its path.
 *
 * Forces @p value and, if it is a derivation, returns a newly allocated
 * StorePath for its `.drvPath`. If the value is not a derivation, NULL is
 * returned without setting an error. If forcing the value raises an assertion
 * failure and @p ignoreAssertionFailures is true, the assertion failure is
 * treated as "not a derivation" (NULL returned, no error); if false, the
 * assertion is propagated as an error.
 *
 * @param[out] context Optional. Stores error information.
 * @param[in]  state   Evaluator state.
 * @param[in]  value   Value to inspect.
 * @param[in]  ignoreAssertionFailures When true, an AssertionError raised while
 *  forcing @p value is treated as "not a derivation" rather than an error.
 * @return A newly allocated StorePath holding the derivation path, or NULL.
 *  Free a non-NULL result with nix_store_path_free().
 */
StorePath *nix_get_derivation(nix_c_context *context, EvalState *state,
                              nix_value *value, bool ignoreAssertionFailures);

/**
 * @brief Call a function, drawing its arguments from an attribute set.
 *
 * Forces @p fn_val and writes its application into @p result. If @p fn_val is
 * a function that expects named arguments, each argument is looked up in @p
 * auto_args; formals with defaults that are absent from @p auto_args use their
 * defaults. If @p auto_args is NULL or not an attribute set, empty bindings are
 * supplied (every formal must then have a default value).
 *
 * @param[out] context   Optional. Stores error information.
 * @param[in]  state     Evaluator state.
 * @param[in]  auto_args Attribute set value supplying named arguments, or NULL.
 * @param[in]  fn_val    The value to call.
 * @param[out] result    Pre-allocated nix_value that receives the result.
 * @return NIX_OK on success, an error code otherwise.
 */
nix_err nix_value_auto_call_function(nix_c_context *context, EvalState *state,
                                     nix_value *auto_args, nix_value *fn_val,
                                     nix_value *result);

#ifdef __cplusplus
}
#endif

#endif /* NIX_API_EXPR_SHIM_H */
