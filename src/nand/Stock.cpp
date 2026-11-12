// SPDX-License-Identifier: GPL-2.0-only
#include <Factory.hpp>
#include <stdexcept>

namespace factory {
  auto stock_titles(const QString& area) -> QMap<quint64, quint16> {
    const char region = area == "EUR" ? 'P' : area == "USA" ? 'E' : area == "JPN" ? 'J' : 0;
    if (!region) throw std::runtime_error("Stock 4.3 supports EUR, USA and JPN; Korean-key titles are not supported");
    // stock-4.3-r1: normalized from WiiQt's pinned List43* tables, not a console inventory or 'latest'.
    // Includes IOS34 v3608 in USA (the upstream table repeats IOS33 instead), and GetUpdate's two channel stubs.
    QMap<quint64, quint16> titles{
      {0x100000004ull, 65280}, {0x100000009ull, 1034}, {0x10000000aull, 768}, {0x10000000bull, 256},
      {0x10000000cull, 526}, {0x10000000dull, 1032}, {0x10000000eull, 1032}, {0x10000000full, 1032},
      {0x100000010ull, 512}, {0x100000011ull, 1032}, {0x100000014ull, 256}, {0x100000015ull, 1039},
      {0x100000016ull, 1294}, {0x10000001cull, 1807}, {0x10000001eull, 2816}, {0x10000001full, 3608},
      {0x100000021ull, 3608}, {0x100000022ull, 3608}, {0x100000023ull, 3608}, {0x100000024ull, 3608},
      {0x100000025ull, 5663}, {0x100000026ull, 4124}, {0x100000028ull, 3072}, {0x100000029ull, 3607},
      {0x10000002bull, 3607}, {0x10000002dull, 3607}, {0x10000002eull, 3607}, {0x100000030ull, 4124},
      {0x100000032ull, 5120}, {0x100000033ull, 4864}, {0x100000034ull, 5888}, {0x100000035ull, 5663},
      {0x100000037ull, 5663}, {0x100000038ull, 5662}, {0x100000039ull, 5919}, {0x10000003aull, 6176},
      {0x10000003cull, 6400}, {0x10000003dull, 5662}, {0x100000046ull, 6912}, {0x100000050ull, 6944},
      {0x1000000deull, 65280}, {0x1000000dfull, 65280}, {0x1000000f9ull, 65280}, {0x1000000faull, 65280},
      {0x1000000feull, 65280}, {0x100000100ull, 6}, {0x100000101ull, 10},
      {0x1000248414141ull, 2}, {0x1000248414241ull, 20}, {0x1000248414341ull, 6},
      {0x1000248414641ull, 3}, {0x1000248414741ull, 3}, {0x1000248415941ull, 3}
    };
    titles.insert(0x100000002ull, region == 'P' ? 514 : region == 'E' ? 513 : 512);
    titles.insert(0x1000248414600ull | region, 7);
    titles.insert(0x1000248414700ull | region, 7);
    titles.insert(0x1000848414b00ull | region, 3);
    titles.insert(0x1000848414c00ull | region, 2);
    // The Japanese update also carries HCCJ, unlike EUR/USA.
    if (region == 'J') titles.insert(0x100084843434aull, 1);
    return titles;
  }
}
