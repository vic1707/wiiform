// SPDX-License-Identifier: GPL-2.0-only
#include <Factory.hpp>
#include <Titles.hpp>
#include <tiktmd.h>

#include <QEventLoop>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QTemporaryFile>
#include <QTimer>
#include <algorithm>
#include <stdexcept>

namespace {
  void require(bool condition, const QString& message) {
    if (!condition) throw std::runtime_error(message.toStdString());
  }

  auto read(const QString& path, qint64 limit) -> QByteArray {
    QFile file(path);
    const QFileInfo info(path);
    require(info.isFile() && !info.isSymLink() && file.open(QIODevice::ReadOnly), "Cannot read cache file: " + path);
    require(file.size() >= 0 && file.size() <= limit, "Cache file exceeds size limit: " + path);
    const auto size = file.size();
    const auto data = file.read(limit + 1);
    require(file.error() == QFileDevice::NoError && data.size() == size, "Incomplete cache read: " + path);
    return data;
  }

  void directory(const QString& path, bool create) {
    const QFileInfo info(path);
    require(!info.isSymLink(), "Cache directory must not be a symlink: " + path);
    if (info.exists()) require(info.isDir(), "Cache path is not a directory: " + path);
    else if (create)
      require(QDir(info.absolutePath()).mkdir(info.fileName()) &&
                  QFile::setPermissions(path, QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner),
              "Cannot create cache directory (parent must exist): " + path);
  }
}

namespace factory {
  auto fetch(const QUrl& url, qint64 limit) -> QByteArray {
    require((url.scheme() == "http" || url.scheme() == "https") && limit >= 0 && limit <= 0x20000040,
            "Invalid bounded HTTP request");
    QNetworkAccessManager manager;
    QNetworkRequest request(url);
    request.setRawHeader("User-Agent", "wii libnup/1.0");
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute, QNetworkRequest::ManualRedirectPolicy);
    auto* reply = manager.get(request);
    reply->setReadBufferSize(std::min<qint64>(limit + 1, 64 * 1024));
    QEventLoop loop;
    QTimer timer;
    timer.setSingleShot(true);
    QByteArray data;
    bool oversized = false, timed_out = false;
    auto drain = [&] {
      if (oversized || timed_out || !reply->bytesAvailable()) return;
      if (reply->bytesAvailable() > limit - data.size()) {
        oversized = true;
        reply->abort();
      } else data += reply->readAll();
    };
    QObject::connect(reply, &QNetworkReply::readyRead, &loop, drain);
    QObject::connect(reply, &QNetworkReply::finished, &loop, &QEventLoop::quit);
    QObject::connect(&timer, &QTimer::timeout, &loop, [&] { timed_out = true; reply->abort(); });
    // shortcut: one 60-second attempt per file; add retries/resume if slow connections require them.
    timer.start(60000);
    if (!reply->isFinished()) loop.exec();
    timer.stop();
    drain();
    require(!oversized, "Download exceeds size limit: " + url.toString());
    require(!timed_out, "Download timed out: " + url.toString());
    const auto status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
    require(!status || status == 200, "HTTP " + QString::number(status) + " fetching " + url.toString());
    require(reply->error() == QNetworkReply::NoError && status == 200,
            "Download failed: " + reply->errorString() + " (" + url.toString() + ")");
    return data;
  }

  auto read_title(const QString& cache, quint64 id, int version, bool offline) -> NusJob {
    require(!cache.trimmed().isEmpty(), "Cache directory must not be empty");
    require(id && version >= 0 && version < 65535, "A concrete title ID/version is required; latest is forbidden");
    const auto text = QString("%1").arg(id, 16, 16, QChar('0'));
    const auto root = QDir(cache).absolutePath();
    const QStringList directories{root, root + "/" + text.left(8), root + "/" + text.left(8) + "/" + text.mid(8),
                                  root + "/" + text.left(8) + "/" + text.mid(8) + "/v" + QString::number(version)};
    for (const auto& path : directories) directory(path, false);
    const auto base = directories.back() + "/";
    QMap<QString, QByteArray> pending;
    auto load = [&](const QString& name, qint64 limit) {
      const auto path = base + name;
      if (QFileInfo::exists(path) || QFileInfo(path).isSymLink()) return read(path, limit);
      if (offline) throw std::runtime_error("Offline mode: required title content missing from cache");
      // NUS uses HTTP. Trust comes from Nintendo signatures and signed content hashes, never transport alone.
      const auto bytes = fetch(QUrl("http://nus.cdn.shop.wii.com/ccs/download/" + text + "/" + name), limit);
      pending[name] = bytes;
      return bytes;
    };
    const auto metadata = load("tmd." + QString::number(version), 2 * 1024 * 1024);
    const auto ticket = load("cetk", 64 * 1024);
    require(check_title_metadata(metadata, ticket), "Malformed title metadata: " + text);
    require(static_cast<uchar>(ticket[0x1f1]) == 0, "WiiQt does not support this ticket's common-key index");
    Tmd tmd(metadata);
    Ticket tik(ticket, false);
    require(tmd.Tid() == id && tik.Tid() == id && tmd.Version() == version, "Title ID/version mismatch: " + text);
    require(check_cert_chain(tmd.Data()) == ERROR_SUCCESS && check_cert_chain(tik.Data()) == ERROR_SUCCESS,
            "Title is not genuinely Nintendo-signed (fakesigning is forbidden): " + text);
    NusJob job;
    job.tid = id;
    job.version = version;
    job.decrypt = false;
    job.data << tmd.Data() << tik.Data();
    quint64 total = 0;
    for (int i = 0; i < tmd.Count(); ++i) {
      const auto size = tmd.Size(i);
      require(size <= 0x20000000 && (total += size) <= 0x20000000, "Title exceeds NAND capacity");
      const auto bytes = load(tmd.Cid(i), (size + 63) & ~quint64(63));
      require(quint64(bytes.size()) >= ((size + 15) & ~quint64(15)) && bytes.size() % 16 == 0,
              "Truncated or unaligned encrypted content: " + text);
      job.data << bytes;
    }
    require(decrypt_title(job), "Invalid title contents: " + text);
    // Only authenticated, fully checked titles may add files to the cache. Existing files are never replaced.
    if (!pending.isEmpty()) {
      for (const auto& path : directories) directory(path, true);
      for (auto item = pending.cbegin(); item != pending.cend(); ++item) {
        QTemporaryFile staging(base + ".download-XXXXXX");
        require(staging.open() && staging.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner) &&
                    staging.write(item.value()) == item.value().size() && staging.flush(), "Cannot stage cache file");
        require(staging.rename(base + item.key()), "Cannot publish cache file; destination must be new");
        staging.setAutoRemove(false);
      }
    }
    return job;
  }
}
