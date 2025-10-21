#ifndef LIX_WRAPPER_H
#define LIX_WRAPPER_H

#ifdef __cplusplus
extern "C" {
#endif

// Initialize Lix store system
void lix_wrapper_init();

// Open a store connection
void* lix_wrapper_open_store();

// Parse a store path
void* lix_wrapper_parse_store_path(void* store, const char* path);

// Build a derivation
int lix_wrapper_build_path(void* store, void* path);

// Free allocated strings
void lix_wrapper_free_string(char* s);

// Free allocated pointers
void lix_wrapper_free_pointer(void* ptr);

#ifdef __cplusplus
}
#endif

#endif