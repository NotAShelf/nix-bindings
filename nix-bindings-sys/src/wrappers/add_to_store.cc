// Shim for store-write entry points missing from Nix's C API.
//
// We deliberately include Nix's *internal* public headers so the
// struct layouts of `nix_c_context`, `Store`, and `StorePath` come from
// upstream rather than being copy-pasted here. Touching those layouts
// by hand was the original source of fragility in this file.
//
// NOTE: ugh.

#include <cstddef>
#include <string_view>

#include <nix/store/content-address.hh>
#include <nix/store/store-api.hh>
#include <nix/util/serialise.hh>

#include <nix_api_store.h>
#include <nix_api_store_internal.h>
#include <nix_api_util.h>
#include <nix_api_util_internal.h>

#include "nix_api_store_text.h"

nix_err nix_store_add_bytes_to_store(nix_c_context *context, Store *store,
                                     const char *name,
                                     const unsigned char *data, size_t data_len,
                                     StorePath **out_path) {
  if (context)
    context->last_err_code = NIX_OK;

  if (store == nullptr)
    return nix_set_err_msg(context, NIX_ERR_UNKNOWN, "store is null");
  if (name == nullptr)
    return nix_set_err_msg(context, NIX_ERR_UNKNOWN, "name is null");
  if (data == nullptr && data_len > 0)
    return nix_set_err_msg(context, NIX_ERR_UNKNOWN, "data is null");
  if (out_path == nullptr)
    return nix_set_err_msg(context, NIX_ERR_UNKNOWN, "out_path is null");

  try {
    nix::StringSource source(
        std::string_view(reinterpret_cast<const char *>(data), data_len));

    auto path = store->ptr->addToStoreFromDump(
        source, std::string_view(name), nix::FileSerialisationMethod::Flat,
        {nix::ContentAddressMethod::Raw::Text}, nix::HashAlgorithm::SHA256,
        nix::StorePathSet(), nix::RepairFlag::NoRepair);

    *out_path = new StorePath{std::move(path)};
    return NIX_OK;
  }
  NIXC_CATCH_ERRS
}

nix_err nix_store_path_to_string(nix_c_context *context, Store *store,
                                 const StorePath *path,
                                 nix_get_string_callback callback,
                                 void *user_data) {
  if (context)
    context->last_err_code = NIX_OK;

  if (store == nullptr)
    return nix_set_err_msg(context, NIX_ERR_UNKNOWN, "store is null");
  if (path == nullptr)
    return nix_set_err_msg(context, NIX_ERR_UNKNOWN, "path is null");
  if (callback == nullptr)
    return nix_set_err_msg(context, NIX_ERR_UNKNOWN, "callback is null");

  try {
    auto rendered = store->ptr->printStorePath(path->path);
    callback(rendered.c_str(),
             static_cast<unsigned int>(rendered.size()), user_data);
    return NIX_OK;
  }
  NIXC_CATCH_ERRS
}
