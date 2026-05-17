#ifndef NIX_API_STORE_TEXT_H
#define NIX_API_STORE_TEXT_H

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @brief Add text content to the store as a flat file.
 *
 * The text is stored content-addressed (text ingestion, SHA-256).
 * The resulting store path has no references.
 *
 * @param[out] context Optional, stores error information
 * @param[in] store Nix store reference
 * @param[in] name Name for the store path (e.g. "my-file.txt")
 * @param[in] text Content to store
 * @param[in] text_len Length of the text in bytes
 * @param[out] out_path The resulting StorePath. Free with nix_store_path_free.
 * @return NIX_OK on success, an error code otherwise.
 */
nix_err nix_store_add_text_to_store(
    nix_c_context * context,
    Store * store,
    const char * name,
    const char * text,
    unsigned int text_len,
    StorePath ** out_path
);

#ifdef __cplusplus
}
#endif

#endif // NIX_API_STORE_TEXT_H
