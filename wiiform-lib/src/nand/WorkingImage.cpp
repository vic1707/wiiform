// SPDX-License-Identifier: GPL-2.0-only
#include <WorkingImage.hpp>
#include <tools.h>

#include <algorithm>
#include <stdexcept>
#include <string>

namespace {
  auto count_fst_entries(NandBin& nand, quint16 entry) -> int {
    int count = 1;
    for (const auto& child : nand.Children(entry)) count += count_fst_entries(nand, child.index);
    return count;
  }
}

namespace working_image {
  Image::Image(std::span<std::uint8_t> image) : device(image) {
    if (image.size() != 0x21000400)
      throw std::runtime_error("Expected a footer-bearing NAND buffer");
    layout = inspect::validate_layout(image, false);
    if (!nand.SetDevice(&device) || !nand.InitNand())
      throw std::runtime_error("Cannot open NAND filesystem for editing");
    if (!nand.CheckHmacMeta(layout.active_metadata))
      throw std::runtime_error("Active NAND metadata HMAC is invalid");
  }

  auto find_path(NandBin& nand, const QString& path) -> std::optional<NandEntry> {
    NandEntry entry;
    if (!nand.Stat(path, &entry)) return std::nullopt;
    return entry;
  }

  auto read_authenticated_file(NandBin& nand, const std::optional<NandEntry>& entry, quint64 maximum) -> QByteArray {
    if (!entry || !entry->IsFile())
      throw std::runtime_error("Expected NAND file is missing or has the wrong type");
    if (entry->size > maximum) throw std::runtime_error("NAND file exceeds the read limit");
    if (!nand.CheckHmacData(entry->index)) throw std::runtime_error("NAND file HMAC is invalid");
    const auto data = nand.GetFile(entry->index);
    if (quint64(data.size()) != entry->size) throw std::runtime_error("Incomplete NAND file read");
    return data;
  }

  auto read_authenticated_file(NandBin& nand, const QString& path, quint64 maximum) -> QByteArray {
    return read_authenticated_file(nand, find_path(nand, path), maximum);
  }

  void require_capacity(NandBin& nand, int orphan_entries,
                        int needed_entries, quint64 needed_clusters,
                        int released_entries, quint64 released_clusters) {
    constexpr int fst_slot_count = 0x17ff;
    const auto used_entries = count_fst_entries(nand, 0) + orphan_entries;
    if (released_entries < 0 || released_entries > used_entries ||
        used_entries - released_entries > fst_slot_count ||
        needed_entries > fst_slot_count - (used_entries - released_entries))
      throw std::runtime_error("Insufficient free NAND FST entries");
    const auto fats = nand.GetFats();
    if (fats.size() != 0x8000) throw std::runtime_error("Invalid NAND allocation table");
    const auto free_clusters = static_cast<quint64>(std::count(fats.cbegin(), fats.cend(), 0xfffe));
    if (needed_clusters >= free_clusters + released_clusters)
      throw std::runtime_error("Insufficient NAND space (one cluster is reserved)");
  }

  void check_single_cluster_ecc(NandBin& nand, quint16 entry, const char* name) {
    const auto clusters = nand.GetFatsForFile(entry);
    if (clusters.size() != 1)
      throw std::runtime_error(std::string(name) + " must occupy one NAND cluster");
    const quint32 first_page = quint32(clusters.first()) * 8;
    for (quint32 page = first_page; page < first_page + 8; ++page)
      if (!nand.CheckEcc(page)) throw std::runtime_error(std::string(name) + " page ECC is invalid");
  }
}
