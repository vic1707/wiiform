// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <wiiform/types.hpp>

#include <QByteArray>
#include <span>
#include <cstdint>

namespace seed {
  auto extract(std::span<const std::uint8_t> backup, std::span<const std::uint8_t> sidecar)
      -> wiiform::SeedParts;
  auto decode_settings(const QByteArray& raw) -> wiiform::ConsoleSettings;
  auto encode_settings(const wiiform::ConsoleSettings& settings) -> QByteArray;
}
