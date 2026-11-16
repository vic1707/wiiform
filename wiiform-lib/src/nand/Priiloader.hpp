// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <wiiform/types.hpp>
#include <cstdint>
#include <span>

namespace priiloader {
  // Replaces the System Menu boot content with the pinned release, keeping the original
  // as the backup Priiloader boots. Leaves an image that already has this release alone.
  void install(std::span<std::uint8_t> image, std::span<const std::uint8_t> app);
  // The executable and launch options of Priiloader's "file" destinations.
  void install_boot_file(std::span<std::uint8_t> image, std::span<const std::uint8_t> dol, bool hardware_access);
  // Both need an installed Priiloader.
  void configure(std::span<std::uint8_t> image, const wiiform::PriiloaderSettings& settings);
  void set_hacks(std::span<std::uint8_t> image, std::span<const std::uint8_t> hack_definitions,
                 std::span<const wiiform::Hack> enabled_hacks);
}
