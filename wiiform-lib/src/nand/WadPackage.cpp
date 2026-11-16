// SPDX-License-Identifier: GPL-2.0-only
#include <WadPackage.hpp>
#include <Support.hpp>

#include <Titles.hpp>
#include <tiktmd.h>

#include <QCryptographicHash>
#include <QtEndian>
#include <stdexcept>
#include <cstring>

namespace {
  constexpr quint64 wad_header_size = 0x20;
  constexpr quint64 section_alignment = 0x40;
  constexpr quint64 max_wad_size = 0x21000000;
  constexpr quint64 max_content_size = 0x20000000;
  constexpr int ticket_size = 0x2a4;
  constexpr int tmd_header_size = 0x1e4;
  constexpr int content_record_size = 0x24;

  auto align(quint64 size) -> quint64 {
    return (size + section_alignment - 1) & ~(section_alignment - 1);
  }

  auto u32(const QByteArray& bytes, int offset) -> quint32 {
    return qFromBigEndian<quint32>(bytes.constData() + offset);
  }

  auto is_wii_ios(quint64 title_id) -> bool {
    const auto slot = static_cast<quint32>(title_id);
    return title_id >> 32 == 0x00000001 && slot >= 3 && slot <= 253;
  }

  struct Rules {
    // The ticket and TMD carry genuine Nintendo signatures.
    bool nintendo_signed;
    // Community packages may select a common key that does not exist while
    // encrypting with the Wii one.
    bool legacy_key_index;
  };

  auto valid_cios_header(const QByteArray& content, std::uint8_t expected_base) -> bool {
    // d2x's ciosmaps.xml writes IOSINFO at the start of the first local module.
    return content.size() >= 0x30 && u32(content, 0x00) == 0x1ee7c105 &&
        u32(content, 0x04) == 1 && u32(content, 0x08) != 0 &&
        u32(content, 0x0c) == expected_base &&
        content.mid(0x10, 4) == QByteArray("d2x\0", 4);
  }

  // Parses and validates what every WAD has in common. `accept_title` is the
  // caller's own rule about which title this may be; it throws to refuse one.
  template<class AcceptTitle>
  auto read_package(std::span<const std::uint8_t> input, Rules rules,
                    AcceptTitle&& accept_title) -> ValidatedTitleJob {
    require(input.size() >= wad_header_size && input.size() <= max_wad_size,
            "WAD file size is outside supported bounds");
    // QByteArray::fromRawData is a view whose lifetime is bounded by this call.
    // Content buffers copied into NusJob remain owned by the job.
    const auto bytes = QByteArray::fromRawData(reinterpret_cast<const char*>(input.data()),
                                               static_cast<int>(input.size()));
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
    require(CheckTitleMetadata(tmd_bytes, ticket_bytes), "Invalid WAD ticket or TMD");
    const auto count = qFromBigEndian<quint16>(tmd_bytes.constData() + 0x1de);
    require(tmd_size == quint32(tmd_header_size + count * content_record_size),
            "WAD TMD size does not match its content table");
    require(u32(ticket_bytes, 0x1d8) == 0, "Console-bound WAD tickets are unsupported");

    // The vWii marker follows the three version bytes in the signed TMD header.
    require(tmd_bytes[0x183] == '\0', "vWii WADs are unsupported");
    Tmd tmd(tmd_bytes);
    accept_title(tmd.Tid(), tmd.Version());
    const int key_index = static_cast<uchar>(ticket_bytes[0x1f1]);
    require(key_index == 0 || (rules.legacy_key_index && key_index > 2),
            "Unsupported WAD ticket common-key index");
    if (rules.nintendo_signed)
      require(check_cert_chain(tmd_bytes) == ERROR_SUCCESS && check_cert_chain(ticket_bytes) == ERROR_SUCCESS,
              "WAD ticket and TMD must be genuinely Nintendo-signed");
    if (key_index != 0) {
      require(check_cert_chain(ticket_bytes) != ERROR_SUCCESS, "Cannot normalize a Nintendo-signed WAD ticket");
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
    return validate_title(std::move(job), QStringLiteral("WAD content validation failed"));
  }

}

namespace wad_package {
  auto read(std::span<const std::uint8_t> input) -> ValidatedTitleJob {
    return read_package(input, {.nintendo_signed = true, .legacy_key_index = false}, [](quint64 title_id, quint16) {
      const auto type = title_id >> 32;
      require(type == 0x00010001 || type == 0x00010002 || is_wii_ios(title_id),
              "WAD install supports Wii channels and IOS slots 3 through 253");
    });
  }

  auto read_unsigned_channel(std::span<const std::uint8_t> input) -> ValidatedTitleJob {
    return read_package(input, {.nintendo_signed = false, .legacy_key_index = true}, [](quint64 title_id, quint16) {
      require(title_id >> 32 == 0x00010001, "Unsigned WADs are supported for downloadable channels only");
    });
  }

  auto read_homebrew_channel(std::span<const std::uint8_t> input) -> ValidatedTitleJob {
    return read_package(input, {.nintendo_signed = false, .legacy_key_index = true}, [](quint64 title_id, quint16) {
      require(wiiform::is_homebrew_channel_title(title_id), "WAD is not a recognized Homebrew Channel title");
    });
  }

  auto read_cios(std::span<const std::uint8_t> input, const wiiform::CiosOptions& options) -> ValidatedTitleJob {
    require(options.slot >= 248 && options.slot <= 251,
            "Supported d2x cIOS slots are 248 through 251");
    require(options.base == 38 || options.base == 56 || options.base == 57 || options.base == 58,
            "Supported d2x cIOS bases are 38, 56, 57, and 58");
    auto validated = read_package(input, {.nintendo_signed = false, .legacy_key_index = false},
                                  [&](quint64 title_id, quint16 version) {
      require(is_wii_ios(title_id) && static_cast<quint32>(title_id) == options.slot,
              "WAD does not match requested cIOS slot");
      require(version == options.revision, "WAD does not match requested cIOS revision");
    });
    const auto& job = validated.get();
    require(Tmd(job.data[0]).Type(0) == 1, "d2x IOSINFO must be in the first local TMD content");
    require(valid_cios_header(job.data[2], options.base),
            "WAD does not contain a valid d2x IOSINFO header for the requested base");
    return validated;
  }
}
