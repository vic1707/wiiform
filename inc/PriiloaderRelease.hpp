// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QByteArray>
#include <QCryptographicHash>

namespace priiloader {
  constexpr auto release = "0.10.0";
  constexpr int release_app_size = 747968;
  // Fingerprint of the app in the authenticated salon 0.10.0 installation.
  constexpr auto release_app_hash = "f4508801cc4274bbcb74c07551d591486209c634fbf44f65765685fb19ec0157";

  inline auto is_release_app(const QByteArray& bytes) -> bool {
    return bytes.size() == release_app_size &&
           QCryptographicHash::hash(bytes, QCryptographicHash::Sha256).toHex() == release_app_hash;
  }
}
