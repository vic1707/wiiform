// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QByteArray>
#include <QString>
#include <wiiform/types.hpp>
#include <cstdint>
#include <optional>
#include <span>

namespace wii_sysconf {
  constexpr int size = 0x4000;
  constexpr auto path = "/shared2/sys/SYSCONF";
  // The BT.DINF value: a count of registered Wiimotes, then their addresses and names.
  constexpr int registrations_size = 0x461;

  // Generates SYSCONF. `registrations` is the BT.DINF value, or empty for a console
  // with no registered Wiimote.
  auto create(const wiiform::SystemSettings& settings, const QString& menu_region,
              std::span<const std::uint8_t> registrations = {}) -> QByteArray;
  // The value of one named entry of a SYSCONF image; empty when the image has none.
  auto find_entry(const QByteArray& sysconf, const QString& name) -> std::optional<QByteArray>;
} // namespace wii_sysconf
