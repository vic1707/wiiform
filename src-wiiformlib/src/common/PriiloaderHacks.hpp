// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QByteArray>
#include <QJsonObject>
#include <QMap>
#include <QString>

namespace priiloader {
  constexpr auto hack_definitions_path = "/title/00000001/00000002/data/hackshas.ini";
  constexpr auto hack_states_path = "/title/00000001/00000002/data/hacksh_s.ini";

  struct HackFiles {
    QByteArray definitions;
    QByteArray states;
    QJsonObject selected;
  };

  auto prepare_hacks(const QString& definitions_path, int menu_version,
                     const QMap<QString, bool>& overrides,
                     const QByteArray& previous_states = {}) -> HackFiles;
  auto inspect_hacks(const QByteArray& definitions, const QByteArray& states,
                     int menu_version) -> QJsonObject;
  auto legacy_fix_480p_definitions() -> QByteArray;
}
