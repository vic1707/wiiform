// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QByteArray>
#include <QMap>
#include <QString>

namespace priiloader {
  struct Settings {
    QString autoboot;
    QString return_to;
  };

  // Decodes only the pinned Priiloader 0.10.0 loader.ini layout.
  auto read_settings(const QByteArray& bytes) -> Settings;
  auto default_settings() -> QByteArray;
  auto patch_settings(QByteArray bytes, const QMap<QString, QString>& overrides) -> QByteArray;
} // namespace priiloader
