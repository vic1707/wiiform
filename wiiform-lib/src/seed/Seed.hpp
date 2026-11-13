// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QByteArray>
#include <cstdint>
#include <span>
#include <string>
#include <yaml-cpp/yaml.h>

namespace seed {
  auto extract(std::span<const std::uint8_t> backup, std::span<const std::uint8_t> sidecar) -> std::string;
  auto decode_settings(const QByteArray& raw) -> YAML::Node;
  auto encode_settings(const YAML::Node& settings) -> QByteArray;
}
