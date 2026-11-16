// SPDX-License-Identifier: GPL-2.0-only
#include <Factory.hpp>
#include <nusdownloader.h>
#include <stdexcept>

namespace factory {
  auto stock_titles(const QString& area) -> QMap<quint64, quint16> {
    const auto* update = area == "EUR" ? "4.3e" : area == "USA" ? "4.3u" : area == "JPN" ? "4.3j" : nullptr;
    if (!update) throw std::runtime_error("Stock 4.3 supports EUR, USA and JPN; Korean-key titles are not supported");
    auto titles = NusDownloader::UpdateTitles(update);
    // WiiQt lists HCCJ v2 for 4.3J; the validated Wiiform builds carry v1.
    if (area == "JPN") titles.insert(0x100084843434aull, 1);
    return titles;
  }
}
