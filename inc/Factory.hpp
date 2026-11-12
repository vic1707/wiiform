// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QString>
#include <QUrl>
#include <tools.h>

namespace factory {
  auto stock_titles(const QString& area) -> QMap<quint64, quint16>;
  auto fetch(const QUrl& url, qint64 limit) -> QByteArray;
  auto read_title(const QString& cache, quint64 id, int version, bool offline = false) -> NusJob;
  void verify_image(const QString& path, const QByteArray& keys, const QByteArray& boots,
                    const QByteArray& settings, const QByteArray& uids, const QList<NusJob>& titles);
  void build(const QString& seed, const QString& cache, const QString& destination, bool offline = false);
}
