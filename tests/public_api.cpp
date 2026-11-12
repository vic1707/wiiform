// SPDX-License-Identifier: GPL-2.0-only
#include <wiiform/core.hpp>

#include <iostream>
#include <stdexcept>
#include <string_view>

int main() {
  try {
    if (wiiform::read::stock_titles("EUR").empty()) return 1;
    wiiform::core::install_wad("unused.nand", "unused.wad", "",
                               wiiform::WadPolicy::standard);
  } catch (const std::runtime_error& error) {
    if (std::string_view(error.what()).find("SHA-256") != std::string_view::npos) {
      std::cout << "Public C++ API linked and returned a native error\n";
      return 0;
    }
    std::cerr << error.what() << '\n';
  }
  return 1;
}
