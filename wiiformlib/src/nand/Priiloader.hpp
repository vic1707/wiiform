// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QJsonObject>
#include <QMap>
#include <QString>
#include <optional>

namespace priiloader {
  auto install(const QString& input, const QString& app, const QString& output,
               const QMap<QString, QString>& overrides) -> QJsonObject;
  auto set_settings(const QString& input, const QString& output,
                    const QMap<QString, QString>& overrides,
                    const QString& hack_definitions,
                    const QMap<QString, bool>& hack_overrides) -> QJsonObject;
}
