// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QString>
#include <cstdint>
#include <span>

namespace inspect {
  struct LayoutSummary {
    quint16 active_metadata = 0;
    int unowned_clusters = 0;
    int reserved_clusters = 0;
    int orphan_entries = 0;
  };

  // Validates the selected raw SFFS layout before WiiQt builds its recursive
  // FST tree. Returns the exact supercluster FindSuperblock() will select and
  // counts non-live historic allocation markers for explicit caller review.
  // Throws fixed, payload-free errors on malformed metadata or allocation maps.
  auto validate_layout(std::span<const std::uint8_t> image, bool generated) -> LayoutSummary;
} // namespace inspect
