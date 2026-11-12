// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QByteArray>
#include <QJsonObject>
#include <QMap>
#include <QString>

namespace priiloader {
  // Decodes only the pinned Priiloader 0.10.0 loader.ini layout. Unknown
  // versions and malformed values are reported without exposing file bytes.
  auto inspect_settings(const QByteArray& bytes) -> QJsonObject;
  auto default_settings() -> QByteArray;
  auto patch_settings(QByteArray bytes, const QMap<QString, QString>& overrides) -> QByteArray;
} // namespace priiloader
