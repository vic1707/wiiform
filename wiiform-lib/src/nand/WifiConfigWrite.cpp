// SPDX-License-Identifier: GPL-2.0-only
#include <WifiConfigWrite.hpp>
#include <WifiConfig.hpp>
#include <Support.hpp>
#include <WorkingImage.hpp>
#include <tools.h>

#include <vector>

namespace wifi_config {
  void initialize(std::span<std::uint8_t> image, std::span<const std::uint8_t> config) {
    require(config.size() == std::size_t(size), "Wi-Fi profile file must be 7004 bytes");
    working_image::Image working(image);
    auto& nand = working.nand;
    require(!working_image::find_path(nand, path), "NAND already has a Wi-Fi profile file");
    const auto parent = working_image::find_path(nand, "/shared2/sys");
    require(parent && parent->IsDir(), "Working image needs /shared2/sys");
    // Each missing directory is created after the one above it.
    std::vector<QString> missing;
    for (const auto* directory : {"/shared2/sys/net", "/shared2/sys/net/02"}) {
      const auto found = working_image::find_path(nand, directory);
      if (found) require(found->IsDir(), "Wi-Fi profile directory is a file");
      else missing.emplace_back(directory);
    }
    working_image::require_capacity(nand, working.layout.orphan_entries, int(missing.size()) + 1, 1);
    for (const auto& directory : missing)
      require(nand.CreateEntry(directory, owner, group, NAND_DIR, NAND_RW, NAND_RW, NAND_RW) != 0,
              "Cannot create a Wi-Fi profile directory");
    const QByteArray bytes(reinterpret_cast<const char*>(config.data()), int(config.size()));
    const auto id = nand.CreateEntry(path, owner, group, NAND_FILE, NAND_RW, NAND_RW, NAND_RW);
    require(id && nand.SetData(id, bytes) && nand.WriteMetaData(), "Cannot persist the Wi-Fi profile file");
    const auto layout = inspect::validate_layout(image, false);
    nand_buffer::Device device{std::span<const std::uint8_t>(image)};
    NandBin reopened;
    require(reopened.SetDevice(&device) && reopened.InitNand() && reopened.CheckHmacMeta(layout.active_metadata),
            "Cannot authenticate the edited NAND buffer");
    const auto entry = working_image::find_path(reopened, path);
    require(working_image::read_authenticated_file(reopened, entry, size) == bytes,
            "Wi-Fi profile readback differs from the requested bytes");
    working_image::check_single_cluster_ecc(reopened, entry->index, "Wi-Fi profile file");
  }
}
