// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <SysConf.hpp>
#include <QString>

namespace sysconf {
  auto set_display(const QString& image, const DisplayOverrides& overrides,
                   const std::optional<Source>& source = std::nullopt) -> QJsonObject;
  auto set_settings(const QString& image, const SettingsOverrides& overrides,
                    const std::optional<Source>& source = std::nullopt) -> QJsonObject;
}
