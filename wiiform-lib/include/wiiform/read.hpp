// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <wiiform/types.hpp>

namespace wiiform::read {
  // Call on the QCoreApplication thread. If no application exists, make the
  // first call on the main thread so Wiiform can create one there.
  // Overlapping calls fail because WiiQt uses a process-wide AES schedule.
  // Provide exactly 1024 sidecar bytes for a footerless backup.
  auto extract_seed(std::span<const std::uint8_t> nand,
                    std::span<const std::uint8_t> sidecar_keys = {}) -> SeedParts;
}
