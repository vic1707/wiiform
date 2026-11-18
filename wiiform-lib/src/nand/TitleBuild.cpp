// SPDX-License-Identifier: GPL-2.0-only
#include <TitleBuild.hpp>
#include <Support.hpp>

#include <tiktmd.h>

#include <QCryptographicHash>
#include <QtEndian>

namespace {
  constexpr int tmd_header_size = 0x1e4;
  constexpr int content_record_size = 0x24;
}

namespace title_build {
  auto tmd(const QByteArray& stock_tmd, const QList<Content>& contents, quint16 boot_position) -> QByteArray {
    require(stock_tmd.size() >= tmd_header_size && !contents.isEmpty() && boot_position < contents.size(),
            "Cannot rebuild the title metadata");
    auto metadata = stock_tmd.left(tmd_header_size);
    qToBigEndian<quint16>(quint16(contents.size()), reinterpret_cast<uchar*>(metadata.data() + 0x1de));
    qToBigEndian<quint16>(boot_position, reinterpret_cast<uchar*>(metadata.data() + 0x1e0));
    for (int index = 0; index < contents.size(); ++index) {
      const auto& content = contents[index];
      QByteArray record(content_record_size, '\0');
      auto* out = reinterpret_cast<uchar*>(record.data());
      qToBigEndian<quint32>(content.cid, out);
      qToBigEndian<quint16>(quint16(index), out + 4);
      qToBigEndian<quint16>(content.type, out + 6);
      qToBigEndian<quint64>(quint64(content.bytes.size()), out + 8);
      record.replace(16, 20, QCryptographicHash::hash(content.bytes, QCryptographicHash::Sha1));
      metadata += record;
    }
    return metadata;
  }

  auto retitle_tmd(const QByteArray& metadata, quint64 title_id, quint16 version) -> QByteArray {
    Tmd tmd(metadata);
    require(tmd.SetTid(title_id) && tmd.SetVersion(version) && tmd.FakeSign(), "Cannot fakesign the title metadata");
    return tmd.Data();
  }

  auto retitle_ticket(const QByteArray& ticket, quint64 title_id) -> QByteArray {
    Ticket retitled(ticket, false);
    require(retitled.SetTid(title_id) && retitled.FakeSign(), "Cannot fakesign the ticket");
    return retitled.Data();
  }

  auto job(quint64 title_id, quint16 version, const QByteArray& metadata, const QByteArray& ticket,
           const QList<Content>& contents, const QString& failure) -> ValidatedTitleJob {
    NusJob job;
    job.tid = title_id;
    job.version = version;
    job.decrypt = true;
    job.data << metadata << ticket;
    for (const auto& content : contents) job.data << content.bytes;
    return validate_title(std::move(job), failure);
  }
}
