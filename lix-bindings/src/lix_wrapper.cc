#include "../include/lix_wrapper.h"
#include <lix/libstore/globals.hh>
#include <lix/libstore/store-api.hh>
#include <lix/libutil/async.hh>

using namespace nix;

static thread_local AsyncIoRoot* aio = nullptr;

extern "C" {
    void lix_wrapper_init() {
        if (!aio) {
            aio = new AsyncIoRoot();
            initLibStore();
        }
    }

    void* lix_wrapper_open_store() {
        lix_wrapper_init();
        return aio->blockOn(nix::openStore()).release();
    }

    void* lix_wrapper_parse_store_path(void* store, const char* path) {
        auto s = static_cast<Store*>(store);
        try {
            auto parsed = s->parseStorePath(path);
            return new StorePath(parsed);
        } catch (...) {
            return nullptr;
        }
    }

    int lix_wrapper_build_path(void* store, void* path) {
        auto s = static_cast<Store*>(store);
        auto p = static_cast<StorePath*>(path);
        try {
            std::vector<DerivedPath> paths {
                DerivedPath::Built {
                    .drvPath = makeConstantStorePathRef(*p),
                    .outputs = OutputsSpec::Names{"out"}
                }
            };
            aio->blockOn(s->buildPathsWithResults(paths, bmNormal, s));
            return 0;
        } catch (...) {
            return 1;
        }
    }

    void lix_wrapper_free_string(char* s) {
        free(s);
    }

    void lix_wrapper_free_pointer(void* ptr) {
        delete static_cast<Store*>(ptr);
    }
}