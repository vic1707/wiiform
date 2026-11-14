// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <tools.h>
#include <wiiform/types.hpp>
#include <cstdint>
#include <span>

namespace wad_package {
  // Parse bounded caller-owned bytes without copying the complete WAD.
  auto read(std::span<const std::uint8_t> bytes, const wiiform::Sha256& expected_sha256,
            wiiform::WadPolicy policy) -> NusJob;
}
