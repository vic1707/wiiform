// SPDX-License-Identifier: GPL-2.0-only
// Boot2 map layout follows giantpune's WiiQt Blocks0to7 parser.
#include <BootDiagnostics.hpp>
#include <tiktmd.h>
#include <BootFlags.hpp>

#include <Titles.hpp>

#include <QMap>
#include <QSet>
#include <QtEndian>
#include <blocks0to7.h>
#include <nandspare.h>

#include <algorithm>
#include <array>
#include <limits>
#include <optional>
#include <stdexcept>

namespace {
  constexpr int kPageSize = 0x840;
  constexpr int kDataSize = 0x800;
  constexpr int kPagesPerBlock = 64;
  constexpr int kBootAreaSize = 0x108000;
  constexpr int kMapOffset = 0x1f800;
  constexpr int kMapRecordSize = 0x4c;
  const QByteArray kMapMagic = QByteArray::fromHex("26f29a401ee684cf");

  void require(bool ok, const char* message) {
    if (!ok) throw std::runtime_error(message);
  }

  struct MapRecord {
    quint32 generation = 0;
    std::array<quint8, 8> blocks{};
  };

  struct CopyLayout {
    int first;
    MapRecord map;
  };

  auto read_map_record(const QByteArray& boot_block) -> std::optional<MapRecord> {
    const auto first = boot_block.mid(kMapOffset, kMapRecordSize);
    const auto second = boot_block.mid(kMapOffset + kMapRecordSize, kMapRecordSize);
    const auto third = boot_block.mid(kMapOffset + 2 * kMapRecordSize, kMapRecordSize);

    // WiiQt's pinned Blocks0to7 parser accepts a map when its magic is present
    // and at least two of the three complete 0x4c-byte records agree.
    QByteArray accepted;
    if (first.startsWith(kMapMagic) && (first == second || first == third)) accepted = first;
    else if (second.startsWith(kMapMagic) && second == third) accepted = second;
    if (accepted.isEmpty()) return std::nullopt;
    MapRecord result{qFromBigEndian<quint32>(accepted.constData() + 8), {}};
    std::copy_n(accepted.constData() + 12, result.blocks.size(), result.blocks.begin());
    return result;
  }

  int first_block(const MapRecord& map, int last) {
    if (last > 4) {
      if (last == 7) return -1;
      // WiiQt starts the backwards scan at block 7 and increments on a bad
      // block. Reject the case that would make it index past the map.
      require(map.blocks[7] == 0, "Unsupported boot2 backup placement");
      return last == 6 || map.blocks[6] != 0 ? 7 : -1;
    }
    int first = -1;
    for (int block = 0; block < last; ++block) {
      if (map.blocks[block] != 0) continue;
      if (first >= 0) return -1;
      first = block;
    }
    return first;
  }

  void check_copy_bounds(const QByteArray& first) {
    const auto* header = first.constData();
    const auto header_size = qFromBigEndian<quint32>(header);
    const auto content_offset = qFromBigEndian<quint32>(header + 4);
    const quint64 ticket_offset = quint64(header_size) + qFromBigEndian<quint32>(header + 8);
    const auto ticket_size = qFromBigEndian<quint32>(header + 12);
    const auto tmd_size = qFromBigEndian<quint32>(header + 16);
    const quint64 tmd_offset = ticket_offset + ticket_size;
    const quint64 header_end = tmd_offset + tmd_size;
    require(header_size == 0x20 && header_end <= quint64(first.size()) && content_offset >= header_end,
            "Boot2 metadata extends outside its header block");
    const auto ticket = first.mid(ticket_offset, ticket_size);
    const auto tmd = first.mid(tmd_offset, tmd_size);
    require(CheckTitleMetadata(tmd, ticket) && qFromBigEndian<quint16>(tmd.constData() + 0x1de) == 1 &&
                ticket[0x1f1] == '\0', "Malformed boot2 ticket or TMD");
    const auto bytes = qFromBigEndian<quint64>(tmd.constData() + 0x1ec);
    const auto payload_end = quint64(first.size()) + kMapOffset;
    require(bytes > 0 && content_offset < payload_end && bytes <= payload_end - content_offset &&
                ((bytes + 63) & ~quint64(63)) <= payload_end - content_offset,
            "Boot2 content extends outside its copy");
  }
} // namespace

namespace inspect {
  auto verify_boot(const QByteArray& raw, BootPolicy policy) -> QSet<int> {
    require(raw.size() == kBootAreaSize, "Unexpected boot-area size");
    QList<QByteArray> blocks;
    for (int block = 0; block < 8; ++block) {
      QByteArray data;
      for (int page = 0; page < kPagesPerBlock; ++page)
        data += raw.mid((block * kPagesPerBlock + page) * kPageSize, kDataSize);
      blocks << data;
    }

    Blocks0to7 parsed(blocks);
    require(parsed.IsOk() && parsed.Boot1Version() != BOOT_1_UNK, "Unrecognized boot1");
    QMap<int, CopyLayout> layouts;
    for (int last = 1; last < blocks.size(); ++last) {
      const auto map = read_map_record(blocks[last]);
      if (!map) continue;
      const auto first = first_block(*map, last);
      if (first < 0) continue;
      // WiiQt tracks the newest generation in a quint16 initialized to zero.
      require(first > 0 && map->generation > 0 && map->generation <= std::numeric_limits<quint16>::max(),
              "Unsupported boot2 map generation or placement");
      check_copy_bounds(blocks[first]);
      layouts.insert(last, {first, *map});
    }
    require(!layouts.isEmpty(), "No recognized boot2 copies");
    const auto copies = parsed.Boot2Infos();
    require(copies.size() == layouts.size(), "Boot2 copies disagree with validated maps");
    constexpr int valid_signatures = BOOT_2_TMD_SIG_OK | BOOT_2_TIK_SIG_OK;
    constexpr int fake_signatures = BOOT_2_TMD_FAKESIGNED | BOOT_2_TIK_FAKESIGNED;
    const bool stock = policy == BootPolicy::Stock;
    bool primary = false;
    QSet<int> map_blocks;
    for (const auto& copy : copies) {
      const auto layout = layouts.constFind(copy.secondBlock);
      require(layout != layouts.cend() && layout->first == copy.firstBlock &&
                  layout->map.generation == copy.generation &&
                  std::equal(layout->map.blocks.cbegin(), layout->map.blocks.cend(), copy.blockMap),
              "Boot2 copy disagrees with its validated map");
      require(copy.state >= 0 && !boot::has_bad_content_hash(copy.state), "Invalid boot2 content or metadata");
      if (stock)
        require((copy.state & valid_signatures) == valid_signatures && !(copy.state & fake_signatures),
                "Modified, invalid or unrecognized boot2");
      primary |= (copy.state & BOOT_2_USED_TO_BOOT) && !(copy.state & BOOT_2_MARKED_BAD);
      map_blocks.insert(copy.secondBlock);
    }
    require(primary, stock ? "No usable signed primary boot2" : "No usable primary boot2");
    require(stock || map_blocks.size() >= 2, "Source boot preservation requires redundant boot2 maps");
    return map_blocks;
  }
} // namespace inspect

namespace inspect {
  void verify_boot_ecc(const QByteArray& boots, BootPolicy policy, const QSet<int>& map_blocks) {
    require(boots.size() == kBootAreaSize, "Unexpected boot-area size");
    QList<int> mismatches;
    for (int page = 0; page < 512; ++page) {
      const auto raw = boots.mid(page * kPageSize, kPageSize);
      const auto data = raw.left(kDataSize);
      if (data == QByteArray(kDataSize, '\xff')) continue;
      if (NandSpare::CalcEcc(data) != raw.mid(0x830, 0x10)) mismatches << page;
    }
    if (mismatches.isEmpty()) return;

    require(map_blocks.size() >= 2 &&
                (policy != BootPolicy::Stock || mismatches.size() == map_blocks.size()),
            "Boot-page ECC mismatches require review");
    for (const auto page : mismatches) {
      const auto ecc = boots.mid(page * kPageSize + 0x830, 0x10);
      require(page % kPagesPerBlock == kPagesPerBlock - 1 &&
                  map_blocks.contains(page / kPagesPerBlock) && ecc == QByteArray(0x10, '\xff'),
              "Boot-page ECC mismatches require review");
    }
  }
} // namespace inspect
