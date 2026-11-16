// SPDX-License-Identifier: GPL-2.0-only
#include <Factory.hpp>
#include <nusdownloader.h>
#include <tiktmd.h>

#include <stdexcept>

namespace {
  void require(bool condition, const QString& message) {
    if (!condition) throw std::runtime_error(message.toStdString());
  }
}

namespace factory {
  auto read_title(const QString& cache, quint64 id, int version, bool offline) -> ValidatedTitleJob {
    require(id && version >= 0 && version < 65535, "A concrete title ID/version is required; latest is forbidden");
    const auto text = QString("%1").arg(id, 16, 16, QChar('0'));
    // An empty cache path keeps everything in memory. NUS uses HTTP: trust comes from
    // Nintendo signatures and signed content hashes, never from the transport or the cache.
    NusDownloader nus(nullptr, cache);
    nus.SetOffline(offline);
    nus.SetRequireSignatures(true);
    NusJob job;
    job.tid = id;
    job.version = static_cast<quint16>(version);
    job.decrypt = true;
    QString error;
    require(NusGetTitleBlocking(nus, job, &error), "Cannot get title " + text + ": " + error);
    return validate_title(std::move(job), "Invalid title contents: " + text);
  }
}
