// SPDX-License-Identifier: GPL-2.0-only
#include <WadPackage.hpp>

#include <Titles.hpp>
#include <tiktmd.h>

#include <QCryptographicHash>
#include <QFile>
#include <QFileInfo>
#include <QtEndian>
#include <cstdio>
#include <stdexcept>
#include <utility>

namespace {
  constexpr quint64 wad_header_size = 0x20;
  constexpr quint64 section_alignment = 0x40;
  constexpr quint64 max_wad_size = 0x21000000;
  constexpr quint64 max_content_size = 0x20000000;
  constexpr int ticket_size = 0x2a4;
  constexpr int tmd_header_size = 0x1e4;
  constexpr int content_record_size = 0x24;

  void require(bool ok, const char* message) {
    if (!ok) throw std::runtime_error(message);
  }

  auto align(quint64 size) -> quint64 {
    return (size + section_alignment - 1) & ~(section_alignment - 1);
  }

  auto u32(const QByteArray& bytes, int offset) -> quint32 {
    return qFromBigEndian<quint32>(bytes.constData() + offset);
  }

  auto read_file(const QString& path) -> QByteArray {
    const QFileInfo info(path);
    QFile file(path);
    require(info.isFile() && !info.isSymLink() && file.open(QIODevice::ReadOnly),
            "WAD must be a regular file");
    const auto size = file.size();
    require(size >= qint64(section_alignment) && size <= qint64(max_wad_size),
            "WAD file size is outside supported bounds");
    const auto bytes = file.read(size + 1);
    require(bytes.size() == size && file.error() == QFileDevice::NoError, "Incomplete WAD read");
    return bytes;
  }

  auto read_stdin() -> QByteArray {
    QFile file;
    require(file.open(stdin, QIODevice::ReadOnly), "Cannot read WAD from standard input");
    QByteArray bytes;
    char buffer[64 * 1024];
    for (;;) {
      const auto size = file.read(buffer, sizeof(buffer));
      require(size >= 0, "Incomplete WAD read from standard input");
      if (size == 0) break;
      require(quint64(bytes.size()) + quint64(size) <= max_wad_size,
              "WAD file size is outside supported bounds");
      bytes.append(buffer, int(size));
    }
    require(quint64(bytes.size()) >= section_alignment, "WAD file size is outside supported bounds");
    return bytes;
  }
}

namespace wad_package {
  auto read(const QString& path, const QByteArray& expected_sha256, Policy policy) -> Package {
    const bool legacy = policy == Policy::LegacyChannel;
    require(!legacy || !expected_sha256.isEmpty(), "Legacy channel WADs require an expected SHA-256");
    require(path != "-" || !expected_sha256.isEmpty(), "WAD input from stdin requires an expected SHA-256");
    const auto expected = expected_sha256.toLower();
    require(expected.isEmpty() || (expected.size() == 64 && QByteArray::fromHex(expected).toHex() == expected),
            "Expected WAD SHA-256 must have 64 hexadecimal digits");
    const auto bytes = path == "-" ? read_stdin() : read_file(path);
    const auto sha256 = QCryptographicHash::hash(bytes, QCryptographicHash::Sha256).toHex();
    require(expected.isEmpty() || sha256 == expected, "WAD SHA-256 does not match the expected package digest");
    require(u32(bytes, 0) == wad_header_size && u32(bytes, 4) == 0x49730000,
            "Expected a standard installable Wii WAD header");
    require(u32(bytes, 0x0c) == 0, "Unsupported WAD reserved header field");

    quint64 offset = section_alignment;
    auto section_offset = [&](quint64 size) {
      const auto start = offset;
      require(start <= quint64(bytes.size()) && size <= quint64(bytes.size()) - start,
              "WAD section extends beyond the file");
      offset += align(size);
      return start;
    };
    section_offset(u32(bytes, 8)); // Embedded certificates do not determine authenticity.
    require(u32(bytes, 0x10) == ticket_size, "Invalid WAD ticket size");
    const auto ticket_offset = section_offset(ticket_size);
    const auto tmd_size = u32(bytes, 0x14);
    require(tmd_size >= tmd_header_size && tmd_size <= tmd_header_size + 0xffff * content_record_size,
            "Invalid WAD TMD size");
    const auto tmd_offset = section_offset(tmd_size);
    const auto data_size = u32(bytes, 0x18);
    const auto data_offset = section_offset(data_size);
    const auto footer_size = u32(bytes, 0x1c);
    const auto footer_offset = section_offset(footer_size);
    const auto end = footer_offset + footer_size;
    require(quint64(bytes.size()) == end || quint64(bytes.size()) == align(end),
            "Unexpected trailing WAD data");

    auto ticket_bytes = bytes.mid(ticket_offset, ticket_size);
    const auto tmd_bytes = bytes.mid(tmd_offset, tmd_size);
    require(check_title_metadata(tmd_bytes, ticket_bytes), "Invalid WAD ticket or TMD");
    const auto count = qFromBigEndian<quint16>(tmd_bytes.constData() + 0x1de);
    require(tmd_size == quint32(tmd_header_size + count * content_record_size),
            "WAD TMD size does not match its content table");
    require(u32(ticket_bytes, 0x1d8) == 0, "Console-bound WAD tickets are unsupported");

    Tmd tmd(tmd_bytes);
    require(!legacy || (tmd.Tid() >> 32) == 0x00010001, "Legacy WAD policy supports downloadable channels");
    const int key_index = static_cast<uchar>(ticket_bytes[0x1f1]);
    require(key_index == 0 || (legacy && key_index > 2), "Unsupported WAD ticket common-key index");
    const auto source_ticket_sha256 = QCryptographicHash::hash(ticket_bytes, QCryptographicHash::Sha256).toHex();
    const bool tmd_signed = check_cert_chain(tmd_bytes) == ERROR_SUCCESS;
    const bool ticket_signed = check_cert_chain(ticket_bytes) == ERROR_SUCCESS;
    if (key_index != 0) {
      require(!ticket_signed, "Cannot normalize a Nintendo-signed WAD ticket");
      // Some legacy packages encrypt with the Wii common key despite an
      // invalid selector. Change the selector in the working ticket only.
      // Every content hash below must verify with that key.
      ticket_bytes[0x1f1] = '\0';
    }
    Ticket ticket(ticket_bytes, false);
    require(tmd.Tid() && ticket.Tid() == tmd.Tid(), "WAD ticket and TMD title IDs do not match");

    NusJob job;
    job.tid = tmd.Tid();
    job.version = tmd.Version();
    job.decrypt = false;
    job.data << tmd_bytes << ticket_bytes;
    const auto padded_data_size = align(data_size);
    quint64 consumed = 0;
    quint64 last_content_end = 0;
    for (int i = 0; i < count; ++i) {
      const auto size = tmd.Size(i);
      require(size > 0 && size <= max_content_size, "Empty or oversized WAD content");
      const auto encrypted_size = align(size);
      require(encrypted_size <= padded_data_size - consumed, "WAD content exceeds its data section");
      job.data << bytes.mid(data_offset + consumed, encrypted_size);
      last_content_end = consumed + size;
      consumed += encrypted_size;
    }
    // Some WAD writers declare the last content's unpadded end. The
    // physical section still reaches the same aligned footer offset.
    require(consumed == padded_data_size && (data_size == consumed || data_size == last_content_end),
            "WAD content sizes do not match its data section");
    require(decrypt_title(job), "WAD content validation failed");

    return {std::move(job), sha256, source_ticket_sha256, key_index, tmd_signed, ticket_signed};
  }
}
