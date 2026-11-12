// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <BuildOptions.hpp>
#include <QJsonObject>
#include <QString>

namespace preflight {
  auto inspect(const QString& seed, const QString& reference, const QString& candidate,
               const nand_build::Options& options = {}) -> QJsonObject;
}
