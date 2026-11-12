// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <string>
#include <string_view>

namespace wiiform::core {
  // Create a disposable WiiQt-backed NAND containing a known smoke-test marker.
  // The destination must not exist. Throws std::exception-derived errors on failure.
  void write_synthetic_nand(std::string_view path);

  // Open a NAND read-only and return its validated smoke-test marker.
  // Throws std::exception-derived errors on missing, invalid, or unexpected data.
  auto inspect_synthetic_nand(std::string_view path) -> std::string;
} // namespace wiiform::core
