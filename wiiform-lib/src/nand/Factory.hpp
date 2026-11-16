// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QString>
#include <tools.h>
#include <TitleValidation.hpp>

namespace factory {
  auto stock_titles(const QString& area) -> QMap<quint64, quint16>;
  // Empty cache path disables disk reads and writes; offline mode then fails
  // on the first requested title file.
  auto read_title(const QString& cache, quint64 id, int version, bool offline) -> ValidatedTitleJob;
}
