// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <wiiform/types.hpp>

#include <QByteArray>
#include <QCryptographicHash>
#include <QString>
#include <cstring>
#include <stdexcept>

// Every rejection in the library is an exception with a plain message.
inline void require(bool condition, const char* message) {
  if (!condition) throw std::runtime_error(message);
}

inline void require(bool condition, const QString& message) {
  if (!condition) throw std::runtime_error(message.toStdString());
}

// A title ID as the 16 hex digits used in NAND paths and NUS URLs.
inline auto title_name(quint64 title_id) -> QString {
  return QString("%1").arg(title_id, 16, 16, QChar('0'));
}

inline auto sha256(const QByteArray& bytes) -> wiiform::Sha256 {
  const auto hash = QCryptographicHash::hash(bytes, QCryptographicHash::Sha256);
  wiiform::Sha256 result;
  std::memcpy(result.data(), hash.constData(), result.size());
  return result;
}
