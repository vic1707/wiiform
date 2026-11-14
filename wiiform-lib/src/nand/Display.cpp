// SPDX-License-Identifier: GPL-2.0-only
#include <Display.hpp>
#include <WorkingImage.hpp>
#include <Seed.hpp>
#include <tools.h>

#include <QTreeWidgetItem>
#include <memory>
#include <stdexcept>

namespace {
  void require(bool ok, const char* message) {
    if (!ok) throw std::runtime_error(message);
  }

  void check_ecc(NandBin& nand, quint16 entry) {
    const auto clusters = nand.GetFatsForFile(entry);
    require(clusters.size() == 1, "SYSCONF must occupy one NAND cluster");
    for (quint32 page = quint32(clusters.first()) * 8; page < quint32(clusters.first()) * 8 + 8; ++page)
      require(nand.CheckEcc(page), "SYSCONF page ECC is invalid");
  }
}

namespace sysconf {
  void initialize(std::span<std::uint8_t> image, const wiiform::SystemSettings& settings) {
    working_image::Image working(image);
    const std::unique_ptr<QTreeWidgetItem> tree(working.nand.GetTree());
    require(!working_image::find_path(tree.get(), path), "NAND already has SYSCONF");
    const auto raw = working_image::read_authenticated_file(working.nand,
        working_image::find_path(tree.get(), "/title/00000001/00000002/data/setting.txt"), 256);
    const auto region = QString::fromStdString(seed::decode_settings(raw).area);
    const auto bytes = create(settings, region);
    const auto* parent = working_image::find_path(tree.get(), "/shared2/sys");
    require(parent && working_image::entry_kind(parent) == NAND_DIR, "Working image needs /shared2/sys");
    working_image::require_capacity(working.nand, tree->child(0), working.layout.orphan_entries, 1, 1);
    // System Menu ownership and permissions observed in original Wii captures.
    const auto id = working.nand.CreateEntry(path, 0x1000, 1, NAND_FILE, NAND_RW, NAND_RW, NAND_RW);
    require(id && working.nand.SetData(id, bytes) && working.nand.WriteMetaData(),
            "Cannot persist system settings");
    const auto layout = inspect::validate_layout(image, false);
    nand_buffer::Device device{std::span<const std::uint8_t>(image)};
    NandBin reopened;
    require(reopened.SetDevice(&device) && reopened.InitNand() && reopened.CheckHmacMeta(layout.active_metadata),
            "Cannot authenticate the edited NAND buffer");
    const std::unique_ptr<QTreeWidgetItem> verified(reopened.GetTree());
    const auto* entry = working_image::find_path(verified.get(), path);
    require(working_image::read_authenticated_file(reopened, entry, size) == bytes,
            "SYSCONF readback differs from the requested settings");
    check_ecc(reopened, working_image::entry_id(entry));
  }
}
