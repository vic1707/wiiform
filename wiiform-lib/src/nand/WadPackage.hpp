// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <tools.h>
#include <cstdint>
#include <span>

namespace wad_package {
  enum class Policy { Standard, LegacyChannel };

  struct Package {
    NusJob job;
    QByteArray sha256;
    QByteArray source_ticket_sha256;
    int source_common_key_index;
    bool tmd_nintendo_signed;
    bool ticket_nintendo_signed;

    bool nintendo_signed() const { return tmd_nintendo_signed && ticket_nintendo_signed; }
  };

  // Parse bounded caller-owned bytes without copying the complete WAD.
  auto read(std::span<const std::uint8_t> bytes, const QByteArray& expected_sha256,
            Policy policy = Policy::Standard) -> Package;
}
