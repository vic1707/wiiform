// SPDX-License-Identifier: GPL-2.0-only
// SFFS selection and field layout adapted from giantpune's WiiQt nandbin.cpp.
#include <Layout.hpp>
#include <NandBuffer.hpp>

#include <QtEndian>

#include <array>
#include <functional>
#include <stdexcept>
#include <string>

namespace {
  constexpr qint64 nand_bytes = 0x21000000;
  constexpr qint64 footer_bytes = 0x400;
  constexpr qint64 raw_page_bytes = 0x840;
  constexpr int data_page_bytes = 0x800;
  constexpr int data_cluster_bytes = 0x4000;
  constexpr int raw_cluster_bytes = 0x4200;
  constexpr int pages_per_cluster = 8;
  constexpr int metadata_clusters = 0x10;
  constexpr quint16 data_cluster_first = 0x40;
  constexpr quint16 metadata_cluster_first = 0x7f00;
  constexpr quint16 cluster_count = 0x8000;
  constexpr quint16 fst_entry_count = 0x17ff;
  constexpr quint16 fst_entry_last = fst_entry_count - 1;
  constexpr int fst_entry_bytes = 0x20;
  constexpr int metadata_header_bytes = 0x0c;
  constexpr int fat_bytes = 0x10000;
  constexpr int fst_offset = metadata_header_bytes + fat_bytes;
  constexpr int maximum_tree_depth = 256;

  constexpr quint16 fat_reserved = 0xfffc;
  constexpr quint16 fat_bad = 0xfffd;
  constexpr quint16 fat_free = 0xfffe;
  constexpr quint16 fat_end = 0xfffb;
  constexpr quint16 fst_end = 0xffff;

  void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }

  auto read_exact(QIODevice& file, qint64 offset, qint64 size) -> QByteArray {
    require(offset >= 0 && size >= 0 && offset <= file.size() && size <= file.size() - offset,
            "NAND layout read is out of bounds");
    require(file.seek(offset), "Cannot seek NAND layout input");
    const auto bytes = file.read(size);
    require(bytes.size() == size,
            "Incomplete NAND layout input");
    return bytes;
  }

  auto read_data_page(QIODevice& file, quint32 page) -> QByteArray {
    return read_exact(file, qint64(page) * raw_page_bytes, data_page_bytes);
  }

  auto supercluster_data(QIODevice& file, quint16 supercluster) -> QByteArray {
    QByteArray result;
    result.reserve(metadata_clusters * data_cluster_bytes);
    const quint32 first_page = quint32(supercluster) * pages_per_cluster;
    for (int page = 0; page < metadata_clusters * pages_per_cluster; ++page)
      result += read_data_page(file, first_page + static_cast<quint32>(page));
    require(result.size() == metadata_clusters * data_cluster_bytes,
            "Incomplete active metadata supercluster");
    return result;
  }

  struct FstEntry {
    std::string name;
    quint8 attr = 0;
    quint16 sub = fst_end;
    quint16 sibling = fst_end;
    quint32 size = 0;
  };

  auto read_name(const QByteArray& raw, bool root) -> std::string {
    require(raw.size() == 12 && raw[0] != '\0', "Invalid NAND FST name");
    int length = 0;
    while (length < raw.size() && raw[length] != '\0') {
      const auto c = static_cast<unsigned char>(raw[length]);
      require(c >= 0x20 && (root || c != '/') && c != '\\' && c != 0x7f,
              "Invalid NAND FST name");
      ++length;
    }
    for (int i = length; i < raw.size(); ++i)
      require(raw[i] == '\0', "Invalid NAND FST name padding");
    const auto name = raw.left(length).toStdString();
    require(name != "." && name != "..", "Invalid NAND FST name");
    return name;
  }

  auto read_fst_entry(const QByteArray& metadata, quint16 index) -> FstEntry {
    require(index < fst_entry_count, "NAND FST entry is out of bounds");
    const auto offset = fst_offset + int(index) * fst_entry_bytes;
    require(offset >= 0 && offset + fst_entry_bytes <= metadata.size(),
            "NAND FST table is truncated");
    const auto* bytes = reinterpret_cast<const uchar*>(metadata.constData() + offset);
    FstEntry entry;
    entry.name = QByteArray(metadata.constData() + offset, 12).split('\0').front().toStdString();
    entry.attr = bytes[0x0c];
    entry.sub = qFromBigEndian<quint16>(bytes + 0x0e);
    entry.sibling = qFromBigEndian<quint16>(bytes + 0x10);
    entry.size = qFromBigEndian<quint32>(bytes + 0x12);
    return entry;
  }

  auto valid_data_cluster(quint16 cluster) -> bool {
    return cluster >= data_cluster_first && cluster < metadata_cluster_first;
  }

  auto select_supercluster(QIODevice& file) -> quint16 {
    // Pinned WiiQt FindSuperblock() policy for ECC-bearing type-1/type-2 dumps:
    // scan physical superclusters in order, skip non-SFFS magic while increasing
    // rewind, stop at the first non-increasing generation, then apply rewind.
    constexpr quint32 start = 0x20be0000;
    constexpr quint32 end = 0x21000000;
    constexpr quint32 length = 0x42000;
    quint32 offset = 0;
    quint32 current_cluster = metadata_cluster_first;
    quint32 best_generation = 0;
    quint8 rewind = 1;
    for (offset = start, current_cluster = metadata_cluster_first;
         offset < end; offset += length, current_cluster += metadata_clusters) {
      const auto header = read_exact(file, offset, 8);
      if (header.left(4) != QByteArray("SFFS", 4)) {
        ++rewind;
        continue;
      }
      const auto generation = qFromBigEndian<quint32>(reinterpret_cast<const uchar*>(header.constData() + 4));
      if (generation > best_generation) {
        best_generation = generation;
      } else {
        rewind = 1;
        break;
      }
    }
    require(best_generation != 0, "No recognized SFFS supercluster");
    require(current_cluster >= quint32(metadata_cluster_first) + quint32(metadata_clusters) * rewind,
            "Invalid active supercluster selection");
    current_cluster -= quint32(metadata_clusters) * rewind;
    offset -= length * rewind;
    require(offset == current_cluster * raw_cluster_bytes &&
                current_cluster >= metadata_cluster_first && current_cluster <= 0x7ff0 &&
                current_cluster % metadata_clusters == 0,
            "Invalid active supercluster location");
    return static_cast<quint16>(current_cluster);
  }

  auto read_fats(const QByteArray& metadata) -> std::array<quint16, cluster_count> {
    require(metadata.size() >= fst_offset, "Active metadata FAT is truncated");
    std::array<quint16, cluster_count> fats{};
    const auto* bytes = reinterpret_cast<const uchar*>(metadata.constData() + metadata_header_bytes);
    for (quint32 i = 0; i < cluster_count; ++i)
      fats[i] = qFromBigEndian<quint16>(bytes + i * sizeof(quint16));
    return fats;
  }

  auto read_fsts(const QByteArray& metadata) -> std::array<FstEntry, fst_entry_count> {
    constexpr int fst_bytes = int(fst_entry_count) * fst_entry_bytes;
    require(metadata.size() >= fst_offset + fst_bytes, "Active metadata FST is truncated");
    std::array<FstEntry, fst_entry_count> entries{};
    for (quint32 i = 0; i < fst_entry_count; ++i) {
      const int offset = fst_offset + int(i) * fst_entry_bytes;
      const auto* raw = metadata.constData() + offset;
      if (raw[0] == '\0') continue;
      entries[i] = read_fst_entry(metadata, static_cast<quint16>(i));
    }
    return entries;
  }

  auto validate_tree_and_chains(const QByteArray& metadata, const std::array<quint16, cluster_count>& fats,
                                const std::array<FstEntry, fst_entry_count>& entries, bool generated)
      -> inspect::LayoutSummary {
    const auto& root = entries[0];
    require(root.name == "/" && (root.attr & 3) == 2 && root.size == 0 && root.sibling == fst_end,
            "Invalid NAND FST root");

    std::array<bool, fst_entry_count> visited{};
    std::array<bool, cluster_count> live_clusters{};
    inspect::LayoutSummary summary;
    int visited_count = 0;
    std::function<void(quint16, int)> walk = [&](quint16 index, int depth) {
      require(index < fst_entry_count && depth <= maximum_tree_depth,
              "NAND FST traversal exceeds safety limits");
      require(!visited[index], "NAND FST contains a cycle or duplicate entry reference");
      const auto& entry = entries[index];
      read_name(metadata.mid(fst_offset + int(index) * fst_entry_bytes, 12), index == 0);
      require(!entry.name.empty() && ++visited_count <= fst_entry_count,
              "NAND FST references an unused entry");
      visited[index] = true;

      const auto mode = entry.attr & 3;
      require(mode == 1 || mode == 2, "NAND FST contains an unsupported entry type");
      if (mode == 2) {
        require(entry.size == 0, "NAND directory has a nonzero size");
        if (entry.sub != fst_end) {
          require(entry.sub > 0 && entry.sub <= fst_entry_last,
                  "NAND directory child index is out of bounds");
        }
      } else {
        require(entry.size <= static_cast<quint32>(nand_bytes), "NAND file exceeds layout limits");
        const quint64 cluster_bytes = static_cast<quint64>(data_cluster_bytes);
        const quint64 needed = (quint64(entry.size) + cluster_bytes - 1) / cluster_bytes;
        require(needed <= metadata_cluster_first - data_cluster_first,
                "NAND file cluster count exceeds the data area");
        if (!needed) {
          require(entry.sub >= 0xfff0 && (!generated || entry.sub == fat_end),
                  "Empty NAND file has an invalid cluster marker");
        } else {
          require(valid_data_cluster(entry.sub), "NAND file starts outside the data area");
          quint16 cluster = entry.sub;
          for (quint64 i = 0; i < needed; ++i) {
            require(valid_data_cluster(cluster) && !live_clusters[cluster],
                    "NAND file chains overlap or leave the data area");
            live_clusters[cluster] = true;
            const auto next = fats[cluster];
            if (i + 1 == needed) {
              require(next == fat_end, "NAND file chain length does not match its FST size");
            } else {
              require(valid_data_cluster(next), "NAND file chain ends before its FST size");
              cluster = next;
            }
          }
        }
      }

      if (entry.sibling != fst_end) {
        require(entry.sibling > 0 && entry.sibling <= fst_entry_last,
                "NAND FST sibling index is out of bounds");
        walk(entry.sibling, depth + 1);
      }
      if (mode == 2 && entry.sub != fst_end)
        walk(entry.sub, depth + 1);
    };
    walk(0, 0);

    for (quint16 index = 0; index < fst_entry_count; ++index) {
      if (visited[index]) continue;
      const auto raw = metadata.mid(fst_offset + int(index) * fst_entry_bytes, fst_entry_bytes);
      if (raw == QByteArray(fst_entry_bytes, '\0')) continue;
      require(!generated, "Generated NAND contains an unused nonzero FST entry");
      ++summary.orphan_entries;
    }

    for (quint16 cluster = 0; cluster < data_cluster_first; ++cluster)
      require(fats[cluster] == fat_reserved, "Boot-area FAT entries are not reserved");
    for (quint16 cluster = metadata_cluster_first; cluster < cluster_count; ++cluster)
      require(fats[cluster] == fat_reserved, "Metadata supercluster FAT entries are not reserved");

    for (quint16 block = 8; block < 4080; ++block) {
      int bad_markers = 0;
      for (quint16 in_block = 0; in_block < 8; ++in_block)
        bad_markers += fats[block * 8 + in_block] == fat_bad;
      require(bad_markers == 0 || bad_markers == 8,
              "NAND bad-block FAT markers are inconsistent within a block");
    }

    for (quint16 cluster = data_cluster_first; cluster < metadata_cluster_first; ++cluster) {
      const auto next = fats[cluster];
      if (live_clusters[cluster]) continue;
      if (next == fat_free || next == fat_bad) continue;
      // WiiQt documents FFFC as reserved, FFFD as bad, FFFE as free and FFFB
      // as the live-chain terminator. Historic dumps can retain reserved or
      // otherwise unowned markers. Count those for the caller's review instead
      // of treating them as live data; active FST chains were validated above.
      if (next == fat_reserved) ++summary.reserved_clusters;
      else ++summary.unowned_clusters;
      if (generated)
        throw std::runtime_error("Generated NAND contains unowned allocated clusters");
    }
    return summary;
  }
} // namespace

namespace inspect {
  static auto validate_device(QIODevice& file, bool generated) -> LayoutSummary {
    require(file.size() == nand_bytes || file.size() == nand_bytes + footer_bytes,
            "Expected an ECC-bearing NAND dump with optional keys footer");
    const auto selected = select_supercluster(file);
    const auto metadata = supercluster_data(file, selected);
    require(metadata.left(4) == QByteArray("SFFS", 4),
            "Active supercluster magic does not match the selected metadata copy");
    auto fats = read_fats(metadata);
    auto entries = read_fsts(metadata);
    auto summary = validate_tree_and_chains(metadata, fats, entries, generated);
    summary.active_metadata = selected;
    return summary;
  }
  auto validate_layout(std::span<const std::uint8_t> image, bool generated) -> LayoutSummary {
    nand_buffer::Device device(image);
    return validate_device(device, generated);
  }
} // namespace inspect
