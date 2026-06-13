#ifndef NIX_API_STORE_TEXT_H
#define NIX_API_STORE_TEXT_H

#include <stddef.h>

#include <nix_api_store.h>
#include <nix_api_util.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @brief Add raw bytes to the store as a flat, content-addressed file.
 *
 * Equivalent to Nix's `builtins.toFile`, but accepts arbitrary bytes
 * (including embedded NULs). The content is ingested with the "Text"
 * content-addressing method (flat serialisation, SHA-256). The resulting
 * store path has no references.
 *
 * @param[out] context  Optional. Stores error information.
 * @param[in]  store    Nix store reference.
 * @param[in]  name     Name component of the store path (e.g. "my-file.bin").
 * @param[in]  data     Byte buffer to write. May be NULL iff `data_len` is 0.
 * @param[in]  data_len Length of the buffer in bytes.
 * @param[out] out_path On success, set to the newly created StorePath.
 *                      Free with nix_store_path_free.
 * @return NIX_OK on success, otherwise a nix_err describing the failure.
 */
nix_err nix_store_add_bytes_to_store(nix_c_context *context, Store *store,
                                     const char *name,
                                     const unsigned char *data, size_t data_len,
                                     StorePath **out_path);

/**
 * @brief Render a StorePath as its canonical /nix/store/... string.
 *
 * Calls `store->printStorePath(path)` and surfaces the result via the
 * standard get-string callback protocol. Unlike `nix_store_path_name`,
 * which only returns the name component, this returns the full path
 * including the store directory and hash.
 *
 * @param[out] context  Optional. Stores error information.
 * @param[in]  store    Nix store reference whose store directory to use.
 * @param[in]  path     Store path to render.
 * @param[in]  callback Receives the rendered string.
 * @param[in]  user_data Forwarded to the callback verbatim.
 * @return NIX_OK on success, otherwise a nix_err describing the failure.
 */
nix_err nix_store_path_to_string(nix_c_context *context, Store *store,
                                 const StorePath *path,
                                 nix_get_string_callback callback,
                                 void *user_data);

#ifdef __cplusplus
}
#endif

#endif // NIX_API_STORE_TEXT_H
