// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <nandbin.h>
#include <sharedcontentmap.h>
#include <uidmap.h>
#include <tools.h>
#include <TitleValidation.hpp>

#include <array>
#include <cstdint>

namespace wiiform {
  // Recognized title IDs used by supported Homebrew Channel releases. Keep
  // package authorization and Priiloader's autoboot check on the same list.
  inline constexpr std::array<std::uint32_t, 5> homebrew_channel_title_ids = {
      0x48415858, // HAXX
      0x4a4f4449, // JODI
      0xaf1bf516, // AF1BF516
      0x4c554c5a, // LULZ
      0x4f484243, // OHBC
  };

  inline auto is_homebrew_channel_title(std::uint64_t title_id) -> bool {
    if ((title_id >> 32) != 0x00010001) return false;
    const auto low = static_cast<std::uint32_t>(title_id);
    for (const auto id : homebrew_channel_title_ids)
      if (id == low) return true;
    return false;
  }
}

namespace wiiform {
  inline constexpr std::uint64_t bootmii_ios_title_id = 0x00000001000000feull;

  // BootMii as IOS marks its TMD with "BM" in a reserved field (TMD offset
  // 0x1b2, TMD-view offset 50), which is how the Homebrew Channel recognizes it.
  inline auto has_bootmii_marker(const QByteArray& tmd) -> bool {
    return tmd.size() > 0x1b3 && tmd[0x1b2] == 'B' && tmd[0x1b3] == 'M';
  }
}

bool initialize_system(NandBin& nand, const QByteArray& uid_data);
bool install_title(NandBin& nand, UIDmap& uid, SharedContentMap& shared,
                   const ValidatedTitleJob& job);
