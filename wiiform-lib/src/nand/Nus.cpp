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

  bool valid_https(const QUrl& url) {
    return url.isValid() && url.scheme() == "https" && !url.host().isEmpty() &&
           !url.authority().contains('@') && !url.hasFragment();
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
    const bool https = url.scheme() == "https";
    require(https ? valid_https(url) : url.scheme() == "http", "Invalid download URL");
    const auto label = https ? url.host() : url.toString();
    require(limit >= 0 && limit <= 0x21000000, "Invalid download size limit");
    QNetworkAccessManager manager;
    QNetworkRequest request(url);
    request.setRawHeader("User-Agent", "wii libnup/1.0");
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute,
                         https ? QNetworkRequest::UserVerifiedRedirectPolicy
                               : QNetworkRequest::ManualRedirectPolicy);
    request.setMaximumRedirectsAllowed(5);
    auto* reply = manager.get(request);
    reply->setReadBufferSize(std::min<qint64>(limit + 1, 64 * 1024));
    QEventLoop loop;
    QTimer timer;
    timer.setSingleShot(true);
    QByteArray data;
    bool oversized = false, timed_out = false, unsafe_redirect = false;
    QObject::connect(reply, &QNetworkReply::redirected, &loop, [&](const QUrl& target) {
      if (valid_https(reply->url().resolved(target))) {
        reply->redirectAllowed();
      } else {
        unsafe_redirect = true;
        reply->abort();
      }
    });
    auto drain = [&] {
      if (oversized || timed_out || unsafe_redirect || !reply->bytesAvailable()) return;
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
    require(!oversized, "Download exceeds size limit: " + label);
    require(!timed_out, "Download timed out: " + label);
    require(!unsafe_redirect, "Download redirected to a disallowed URL: " + label);
    const auto status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
    if (reply->error() == QNetworkReply::TooManyRedirectsError)
      throw std::runtime_error("Download exceeded five redirects");
    require(!status || status == 200, "HTTP " + QString::number(status) + " fetching " + label);
    const auto error = reply->error();
    require(error == QNetworkReply::NoError && status == 200,
            https ? "HTTPS download failed (Qt network error " + QString::number(error) + "): " + label
                  : "Download failed: " + reply->errorString() + " (" + label + ")");
    return data;
  }

  auto read_title(const QString& cache, quint64 id, int version, bool offline) -> ValidatedTitleJob {
    require(id && version >= 0 && version < 65535, "A concrete title ID/version is required; latest is forbidden");
    const auto text = QString("%1").arg(id, 16, 16, QChar('0'));
    // An empty cache path means fetch into memory only: do not inspect or create
    // any filesystem paths. An explicit path opts into the authenticated cache.
    const bool use_cache = !cache.isEmpty();
    const auto root = use_cache ? QDir(cache).absolutePath() : QString{};
    const QStringList directories = use_cache
                                        ? QStringList{root, root + "/" + text.left(8),
                                                      root + "/" + text.left(8) + "/" + text.mid(8),
                                                      root + "/" + text.left(8) + "/" + text.mid(8) + "/v" +
                                                          QString::number(version)}
                                        : QStringList{};
    for (const auto& path : directories) directory(path, false);
    const auto base = use_cache ? directories.back() + "/" : QString{};
    QMap<QString, QByteArray> pending;
    auto load = [&](const QString& name, qint64 limit) {
      const auto path = base + name;
      if (use_cache && (QFileInfo::exists(path) || QFileInfo(path).isSymLink())) return read(path, limit);
      if (offline) throw std::runtime_error("Offline mode: required title content missing from cache");
      // NUS uses HTTP. Trust comes from Nintendo signatures and signed content hashes, never transport alone.
      const auto bytes = fetch(QUrl("http://nus.cdn.shop.wii.com/ccs/download/" + text + "/" + name), limit);
      if (use_cache) pending[name] = bytes;
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
    auto validated = validate_title(std::move(job), "Invalid title contents: " + text);
    // Only authenticated, fully checked titles may add files to the cache. Existing files are never replaced.
    if (use_cache && !pending.isEmpty()) {
      for (const auto& path : directories) directory(path, true);
      for (auto item = pending.cbegin(); item != pending.cend(); ++item) {
        QTemporaryFile staging(base + ".download-XXXXXX");
        require(staging.open() && staging.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner) &&
                    staging.write(item.value()) == item.value().size() && staging.flush(), "Cannot stage cache file");
        require(staging.rename(base + item.key()), "Cannot publish cache file; destination must be new");
        staging.setAutoRemove(false);
      }
    }
    return validated;
  }
}
