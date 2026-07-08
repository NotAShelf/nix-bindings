#ifndef NIX_API_FLAKE_SHIM_H
#define NIX_API_FLAKE_SHIM_H

#include <nix_api_flake.h>
#include <nix_api_util.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @brief Export a locked flake as an owned JSON graph.
 *
 * The returned JSON contains the lock file and the source paths that Nix keeps
 * alongside the lock graph for local or overridden inputs.
 */
nix_err nix_locked_flake_export_json(nix_c_context *context,
                                     nix_locked_flake *locked_flake,
                                     nix_get_string_callback callback,
                                     void *user_data);

/**
 * @brief Import a locked flake from an owned JSON graph.
 *
 * This reconstructs the locked graph without resolving the flake reference,
 * writing a lock file, or updating inputs. The returned value is intended for
 * nix_locked_flake_get_output_attrs.
 */
nix_locked_flake *nix_locked_flake_import_json(
    nix_c_context *context, nix_fetchers_settings *fetch_settings,
    const char *json, size_t json_len);

#ifdef __cplusplus
}
#endif

#endif /* NIX_API_FLAKE_SHIM_H */
