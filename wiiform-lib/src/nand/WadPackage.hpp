// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <tools.h>
#include <TitleValidation.hpp>
#include <wiiform/types.hpp>
#include <cstdint>
#include <span>

namespace wad_package {
  // Parse bounded caller-owned bytes without copying the complete WAD.
  auto read(std::span<const std::uint8_t> bytes, const wiiform::Sha256& expected_sha256,
            wiiform::WadPolicy policy) -> ValidatedTitleJob;

  // Parse a SHA-pinned Wii d2x cIOS WAD. Its custom TMD/ticket are not
  // Nintendo-signed; the expected whole-package digest is the trust anchor.
  auto read_cios(std::span<const std::uint8_t> bytes, const wiiform::Sha256& expected_sha256,
                 const wiiform::CiosOptions& options) -> ValidatedTitleJob;
}
