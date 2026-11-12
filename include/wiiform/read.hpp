// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <wiiform/types.hpp>

#include <cstdint>
#include <string>
#include <string_view>
#include <vector>

namespace wiiform::read {

  using DisplaySource = wiiform::DisplaySource;

  enum class Inspection { Display, SystemSettings, DisplaySource, Priiloader };

  struct StockTitle {
    std::uint64_t title_id;
    std::uint16_t version;
  };

  // Seed files contain console secrets. These operations only read inputs and
  // publish new, unencrypted outputs; existing output paths are rejected.
  void export_seed(std::string_view nand_backup, std::string_view new_seed);
  void unpack_seed(std::string_view seed, std::string_view new_directory);

  // Rich, payload-free reports are JSON strings so language bindings do not
  // need to mirror Qt's JSON object types.
  auto inventory(std::string_view nand) -> std::string;
  auto preflight(std::string_view seed, std::string_view reference,
                 std::string_view candidate, BootPolicy boot_policy,
                 const std::vector<SettingOverride>& settings) -> std::string;
  auto inspect(std::string_view nand, Inspection scope) -> std::string;
  auto display_source(std::string_view authenticated_reference) -> DisplaySource;

  // The recognized release app is bounded to the pinned Priiloader app size.
  auto priiloader_app(std::string_view nand) -> std::vector<std::uint8_t>;

  // WAD inputs are parsed and content-validated without changing the source.
  // `expected_sha256` can be empty only for the standard inspection policy.
  auto inspect_wad(std::string_view wad, std::string_view expected_sha256,
                   WadPolicy policy = WadPolicy::standard) -> std::string;

  auto stock_titles(std::string_view area) -> std::vector<StockTitle>;

} // namespace wiiform::read
