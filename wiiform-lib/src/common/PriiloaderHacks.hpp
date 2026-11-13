// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QByteArray>
#include <QMap>
#include <QString>
#include <cstdint>
#include <span>

namespace priiloader {
  constexpr auto hack_definitions_path = "/title/00000001/00000002/data/hackshas.ini";
  constexpr auto hack_states_path = "/title/00000001/00000002/data/hacksh_s.ini";

  struct HackFiles {
    QByteArray definitions;
    QByteArray states;
  };

  auto prepare_hacks(std::span<const std::uint8_t> definitions, int menu_version,
                     const QMap<QString, bool>& overrides,
                     const QByteArray& previous_states = {}) -> HackFiles;
  auto legacy_fix_480p_definitions() -> QByteArray;
}
