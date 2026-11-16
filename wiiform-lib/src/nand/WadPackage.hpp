// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <tools.h>
#include <TitleValidation.hpp>
#include <wiiform/types.hpp>
#include <cstdint>
#include <span>

// Each function parses caller-owned bytes without copying the complete WAD,
// and accepts one kind of package. Whether the bytes are the expected ones
// (a pinned hash) is the caller's business.
namespace wad_package {
  // A Nintendo-signed channel or Wii IOS (slots 3 through 253).
  auto read(std::span<const std::uint8_t> bytes) -> ValidatedTitleJob;
  // A downloadable channel that need not be signed.
  auto read_unsigned_channel(std::span<const std::uint8_t> bytes) -> ValidatedTitleJob;
  // A Homebrew Channel release, recognized by its title ID.
  auto read_homebrew_channel(std::span<const std::uint8_t> bytes) -> ValidatedTitleJob;
  // A Wii d2x cIOS for the given slot, base and revision. Its TMD and ticket
  // are not Nintendo-signed.
  auto read_cios(std::span<const std::uint8_t> bytes, const wiiform::CiosOptions& options) -> ValidatedTitleJob;
}
