#include <climits>
#include <map>
#include <string_view>
#include <variant>

#include <nlohmann/json.hpp>

#include <nix/flake/flake.hh>
#include <nix/flake/lockfile.hh>
#include <nix/fetchers/attrs.hh>
#include <nix/util/source-accessor.hh>
#include <nix/util/source-path.hh>

#include <nix_api_expr_internal.h>
#include <nix_api_fetchers_internal.hh>
#include <nix_api_flake_internal.hh>
#include <nix_api_util.h>
#include <nix_api_util_internal.h>

#include "nix_api_flake_shim.h"

static constexpr int EXPORT_VERSION = 1;
static constexpr const char *FIELD_VERSION = "version";
static constexpr const char *FIELD_LOCK_FILE = "lockFile";
static constexpr const char *FIELD_NODE_PATHS = "nodePaths";
static constexpr const char *FIELD_ROOT = "root";
static constexpr const char *FIELD_ORIGINAL = "original";
static constexpr const char *FIELD_RESOLVED = "resolved";
static constexpr const char *FIELD_LOCKED = "locked";
static constexpr const char *FIELD_FORCE_DIRTY = "forceDirty";

static nix::SourcePath source_path_from_json(const nlohmann::json &value) {
  return nix::SourcePath{nix::getFSSourceAccessor(),
                         nix::CanonPath(value.get<std::string>())};
}

static nix::FlakeRef flake_ref_from_json(
    const nix::fetchers::Settings &settings, const nlohmann::json &value) {
  return nix::FlakeRef::fromAttrs(settings, nix::fetchers::jsonToAttrs(value));
}

static void collect_lock_file_nodes(
    const nlohmann::json &nodes_json, const std::string &key,
    nix::ref<nix::flake::Node> node,
    std::map<std::string, nix::ref<nix::flake::Node>> &nodes_by_key) {
  if (!nodes_by_key.emplace(key, node).second)
    return;

  const auto &json_node = nodes_json.at(key);
  auto inputs = json_node.find("inputs");
  if (inputs == json_node.end())
    return;

  for (const auto &input : inputs->items()) {
    if (!input.value().is_string())
      continue;

    auto edge = node->inputs.find(input.key());
    if (edge == node->inputs.end())
      throw nix::Error("lock file node '%s' is missing input '%s'", key,
                       input.key());

    auto child = std::get_if<0>(&edge->second);
    if (!child)
      throw nix::Error("lock file node '%s' input '%s' is not a locked node",
                       key, input.key());

    collect_lock_file_nodes(nodes_json, input.value().get<std::string>(),
                            *child, nodes_by_key);
  }
}

extern "C" {

nix_err nix_locked_flake_export_json(nix_c_context *context,
                                     nix_locked_flake *locked_flake,
                                     nix_get_string_callback callback,
                                     void *user_data) {
  nix_clear_err(context);
  if (!locked_flake || !callback)
    return nix_set_err_msg(context, NIX_ERR_UNKNOWN, "null argument");

  try {
    auto [lock_json, node_keys] = locked_flake->lockedFlake->lockFile.toJSON();
    auto node_paths = nlohmann::json::object();

    for (const auto &[node, source_path] : locked_flake->lockedFlake->nodePaths) {
      auto key = node_keys.find(node);
      if (key == node_keys.end())
        throw nix::Error("locked flake node path has no lock file node");
      node_paths[key->second] = source_path.path.abs();
    }

    nlohmann::json out = {
        {FIELD_VERSION, EXPORT_VERSION},
        {FIELD_LOCK_FILE, std::move(lock_json)},
        {FIELD_NODE_PATHS, std::move(node_paths)},
        {FIELD_ROOT,
         {{FIELD_ORIGINAL,
           nix::fetchers::attrsToJSON(
               locked_flake->lockedFlake->flake.originalRef.toAttrs())},
          {FIELD_RESOLVED,
           nix::fetchers::attrsToJSON(
               locked_flake->lockedFlake->flake.resolvedRef.toAttrs())},
          {FIELD_LOCKED,
           nix::fetchers::attrsToJSON(
               locked_flake->lockedFlake->flake.lockedRef.toAttrs())},
          {FIELD_FORCE_DIRTY, locked_flake->lockedFlake->flake.forceDirty}}},
    };

    auto dumped = out.dump();
    if (dumped.size() > UINT_MAX)
      throw nix::Error("locked flake JSON is too large for the C callback");
    callback(dumped.data(), static_cast<unsigned int>(dumped.size()), user_data);
  }
  NIXC_CATCH_ERRS
}

nix_locked_flake *nix_locked_flake_import_json(
    nix_c_context *context, nix_fetchers_settings *fetch_settings,
    const char *json, size_t json_len) {
  nix_clear_err(context);
  if (!fetch_settings || !json)
    return (nix_set_err_msg(context, NIX_ERR_UNKNOWN, "null argument"),
            nullptr);

  try {
    auto exported =
        nlohmann::json::parse(std::string_view(json, json_len));
    if (exported.value(FIELD_VERSION, 0) != EXPORT_VERSION)
      throw nix::Error("unsupported locked flake export version");

    const auto &lock_file_json = exported.at(FIELD_LOCK_FILE);
    const auto &lock_nodes_json = lock_file_json.at("nodes");
    nix::flake::LockFile lock_file{
        *fetch_settings->settings, lock_file_json.dump(),
        "<locked-flake-export>"};

    std::map<std::string, nix::ref<nix::flake::Node>> nodes_by_key;
    collect_lock_file_nodes(lock_nodes_json,
                            lock_file_json.at("root").get<std::string>(),
                            lock_file.root, nodes_by_key);

    std::map<nix::ref<nix::flake::Node>, nix::SourcePath> node_paths;
    for (const auto &[key, path] : exported.at(FIELD_NODE_PATHS).items()) {
      auto node = nodes_by_key.find(key);
      if (node == nodes_by_key.end())
        throw nix::Error("locked flake export references missing node '%s'",
                         key);
      node_paths.emplace(node->second, source_path_from_json(path));
    }

    if (!node_paths.contains(lock_file.root))
      throw nix::Error("locked flake export is missing root source path");

    auto root = exported.at(FIELD_ROOT);
    auto root_dir = node_paths.at(lock_file.root);
    nix::flake::Flake flake{
        .originalRef =
            flake_ref_from_json(*fetch_settings->settings, root.at(FIELD_ORIGINAL)),
        .resolvedRef =
            flake_ref_from_json(*fetch_settings->settings, root.at(FIELD_RESOLVED)),
        .lockedRef =
            flake_ref_from_json(*fetch_settings->settings, root.at(FIELD_LOCKED)),
        .path = root_dir / "flake.nix",
        .forceDirty = root.value(FIELD_FORCE_DIRTY, false),
    };

    auto locked_flake = nix::make_ref<nix::flake::LockedFlake>(
        nix::flake::LockedFlake{.flake = std::move(flake),
                                .lockFile = std::move(lock_file),
                                .nodePaths = std::move(node_paths)});
    return new nix_locked_flake{locked_flake};
  }
  NIXC_CATCH_ERRS_NULL
}

} // extern "C"
