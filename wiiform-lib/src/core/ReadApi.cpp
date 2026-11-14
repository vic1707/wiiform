// SPDX-License-Identifier: GPL-2.0-only
#include <wiiform/read.hpp>
#include "QtRuntime.hpp"
#include <Seed.hpp>

namespace wiiform::read {
  auto extract_seed(std::span<const std::uint8_t> nand,
                    std::span<const std::uint8_t> sidecar_keys) -> SeedParts {
    core::detail::ensure_qcore_application();
    return seed::extract(nand, sidecar_keys);
  }
}
