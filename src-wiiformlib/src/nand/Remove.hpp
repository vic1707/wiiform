// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QString>

namespace title_remove {
  // Remove an installed Forecast or News Channel title, retaining its data
  // directory, UID record and shared-content map.
  void remove_channel(const QString& image, quint64 title_id);
}
