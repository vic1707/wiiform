// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QJsonObject>
#include <QString>

namespace inventory {
  enum class Scope { Filesystem, Display, DisplaySource, Priiloader, PriiloaderApp };

  // Generic, read-only active-filesystem evidence. No stock-profile or restore approval.
  auto inspect(const QString& image, Scope scope = Scope::Filesystem) -> QJsonObject;
}
