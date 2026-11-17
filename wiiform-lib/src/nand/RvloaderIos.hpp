// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <TitleValidation.hpp>
#include <wiiform/types.hpp>

namespace rvloader {
  // The title RVLoader v2.1's installer writes for one IOS, derived from the stock one.
  auto derive_ios(const ValidatedTitleJob& stock, const wiiform::RvloaderIos& ios) -> ValidatedTitleJob;
}
