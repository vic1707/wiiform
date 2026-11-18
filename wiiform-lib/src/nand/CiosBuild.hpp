// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <TitleValidation.hpp>
#include <wiiform/types.hpp>

namespace cios_build {
  // The cIOS a d2x map describes, built from the stock base IOS.
  auto derive(const ValidatedTitleJob& base, const wiiform::CiosBuild& build) -> ValidatedTitleJob;
}
