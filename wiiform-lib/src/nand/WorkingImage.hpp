// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <Layout.hpp>
#include <NandBuffer.hpp>
#include <nandbin.h>

#include <optional>

namespace working_image {
  class Image {
    nand_buffer::Device device;

  public:
    explicit Image(std::span<std::uint8_t> image);
    NandBin nand;
    inspect::LayoutSummary layout;
  };

  // Looks up the current filesystem state; a result is a copy and does not follow later writes.
  auto find_path(NandBin& nand, const QString& path) -> std::optional<NandEntry>;
  auto read_authenticated_file(NandBin& nand, const std::optional<NandEntry>& entry, quint64 maximum) -> QByteArray;
  auto read_authenticated_file(NandBin& nand, const QString& path, quint64 maximum) -> QByteArray;
  void require_capacity(NandBin& nand, int orphan_entries,
                        int needed_entries, quint64 needed_clusters,
                        int released_entries = 0, quint64 released_clusters = 0);
  // Throws unless the file occupies one cluster whose eight pages all pass their ECC.
  void check_single_cluster_ecc(NandBin& nand, quint16 entry, const char* name);
}
