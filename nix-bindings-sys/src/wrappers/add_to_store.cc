#include <string>
#include <optional>

#include <nix/store/store-api.hh>
#include <nix/store/content-address.hh>
#include <nix/util/serialise.hh>
#include <nix/util/error.hh>

#include <nix_api_store.h>
#include <nix_api_util.h>

extern "C" {

// This must match the actual nix_c_context defined in nix_api_util.cc
struct nix_c_context {
    nix_err last_err_code;
    std::optional<std::string> last_err;
    std::optional<nix::ErrorInfo> info;
    std::string name;
};

struct Store {
    nix::ref<nix::Store> ptr;
};

struct StorePath {
    nix::StorePath path;
};

static nix_err set_err(nix_c_context * context, nix_err code, const char * msg)
{
    if (context) {
        context->last_err_code = code;
        context->last_err = std::string(msg);
    }
    return code;
}

nix_err nix_store_add_text_to_store(
    nix_c_context * context,
    Store * store,
    const char * name,
    const char * text,
    unsigned int text_len,
    StorePath ** out_path)
{
    if (context)
        context->last_err_code = NIX_OK;

    if (store == nullptr)
        return set_err(context, NIX_ERR_UNKNOWN, "store is null");
    if (name == nullptr)
        return set_err(context, NIX_ERR_UNKNOWN, "name is null");
    if (text == nullptr && text_len > 0)
        return set_err(context, NIX_ERR_UNKNOWN, "text is null");
    if (out_path == nullptr)
        return set_err(context, NIX_ERR_UNKNOWN, "out_path is null");

    try {
        nix::StringSource source(std::string_view(text, text_len));

        auto result = store->ptr->addToStoreFromDump(
            source,
            std::string_view(name),
            nix::FileSerialisationMethod::Flat,
            {nix::ContentAddressMethod::Raw::Text},
            nix::HashAlgorithm::SHA256,
            nix::StorePathSet(),
            nix::RepairFlag::NoRepair);

        *out_path = new StorePath{std::move(result)};
        return NIX_OK;
    } catch (std::exception & e) {
        return set_err(context, NIX_ERR_UNKNOWN, e.what());
    } catch (...) {
        return set_err(context, NIX_ERR_UNKNOWN, "unknown error in nix_store_add_text_to_store");
    }
}

} // extern "C"
