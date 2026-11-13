// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <Inspection.hpp>
#include <wiiform/types.hpp>
#include <QStringList>
#include <span>
#include <cstdint>

namespace nand_build {
  struct Profile {
    QString area;
    QByteArray settings;
    QList<quint16> bad_blocks;
  };

  auto resolve_profile(const QByteArray& keys, const QByteArray& settings,
                       std::span<const std::uint16_t> bad_blocks, wiiform::Menu menu) -> Profile;
  void initialize_image(std::span<std::uint8_t> image, const QByteArray& keys, const QByteArray& boot,
                        const Profile& profile, inspect::BootPolicy policy);
} // namespace nand_build
