// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <Layout.hpp>
#include <NandBuffer.hpp>
#include <nandbin.h>

namespace working_image {
  class Image {
    nand_buffer::Device device;

  public:
    explicit Image(std::span<std::uint8_t> image);
    NandBin nand;
    inspect::LayoutSummary layout;
  };

  auto find_path(QTreeWidgetItem* tree, const QString& path) -> QTreeWidgetItem*;
  auto entry_kind(const QTreeWidgetItem* item) -> int;
  auto entry_id(const QTreeWidgetItem* item) -> quint16;
  auto read_authenticated_file(NandBin& nand, const QTreeWidgetItem* item, quint64 maximum) -> QByteArray;
  void require_capacity(NandBin& nand, const QTreeWidgetItem* root, int orphan_entries,
                        int needed_entries, quint64 needed_clusters,
                        int released_entries = 0, quint64 released_clusters = 0);
}
