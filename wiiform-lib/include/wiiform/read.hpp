// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <wiiform/types.hpp>

// Calling rules for every function of the library:
// Call on the QCoreApplication thread. If no application exists, make the first
// call on the main thread so Wiiform can create one there. Overlapping calls
// fail because WiiQt uses a process-wide AES schedule. Failures are exceptions
// derived from std::exception.
namespace wiiform {
  // Provide exactly 1024 sidecar bytes for a footerless backup.
  auto extract_seed(std::span<const std::uint8_t> nand,
                    std::span<const std::uint8_t> sidecar_keys = {}) -> SeedParts;

  // The Nintendo titles that make up a stock console with this System Menu.
  auto stock_titles(Menu menu) -> std::vector<TitleVersion>;

  // Checks a footer-bearing image holding exactly the stock titles for this seed and menu:
  // every page and file authenticates, and the System Menu settings match the seed.
  void verify_stock(std::span<const std::uint8_t> nand, const StockSeed& seed, Menu menu);

  // Checks that an edited stock image still authenticates completely and that
  // its boot area and key footer are the ones initialize_image wrote.
  void verify_generated_nand(std::span<const std::uint8_t> nand, const StockIdentity& expected);
}
