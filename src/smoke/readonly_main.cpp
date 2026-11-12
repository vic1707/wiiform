// SPDX-License-Identifier: GPL-2.0-only
#include <wiiform/core.hpp>

#include <exception>
#include <iostream>
#include <string_view>

int main(int argc, char** argv) {
  if (argc != 3 || std::string_view(argv[1]) != "smoke-inspect") {
    std::cerr << "Usage: wiiform-readonly smoke-inspect NAND_PATH\n";
    return 2;
  }
  try {
    std::cout << wiiform::core::inspect_synthetic_nand(argv[2]) << '\n';
  } catch (const std::exception& error) {
    std::cerr << error.what() << '\n';
    return 1;
  }
  return 0;
}
