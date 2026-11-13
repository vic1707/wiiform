// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <wiiform/types.hpp>

namespace wiiform::read {
  // Provide exactly 1024 sidecar bytes for a footerless backup.
  auto extract_seed(std::span<const std::uint8_t> nand,
                    std::span<const std::uint8_t> sidecar_keys = {}) -> std::string;
}
