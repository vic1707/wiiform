// SPDX-License-Identifier: GPL-2.0-only
#include <wiiform/write.hpp>
#include "QtRuntime.hpp"
#include <Inspection.hpp>
#include <Layout.hpp>
#include <NandBuffer.hpp>

#include <QCryptographicHash>
#include <QRegularExpression>

#include <cstring>
#include <stdexcept>

namespace wiiform::core {
namespace {
  constexpr qint64 footer_bytes = 0x400;
  constexpr qint64 footer_header_bytes = 0x100;
  constexpr qint64 otp_console_id_offset = 0x124;

  void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }

  auto normalize(const QByteArray& footer) -> QByteArray {
    require(footer.size() == footer_bytes, "Invalid BootMii key footer size");
    const auto id = footer.mid(otp_console_id_offset, 4).toHex();
    const auto header = footer.left(footer_header_bytes);
    static const QRegularExpression console_id(
        "(?:^|\\n)Console ID:[ \\t]*([0-9a-fA-F]{8})(?:\\r?$)",
        QRegularExpression::MultilineOption);
    const auto match = console_id.match(QString::fromLatin1(header));
    require(match.hasMatch() && match.captured(1).toLower().toLatin1() == id,
            "nanddumper@IOS footer console ID differs from OTP or is missing");
    const auto text = QByteArray("BackupMii v1, ConsoleID: ") + id + '\n';
    return text.leftJustified(footer_header_bytes, '\0', true) + footer.mid(footer_header_bytes);
  }
}

auto prepare_backupmii(std::span<std::uint8_t> image, const StockIdentity& expected)
    -> std::vector<std::uint8_t> {
  require(image.size() == nand_size, "Expected a footer-bearing NAND buffer");
  const auto digest_matches = [](std::span<const std::uint8_t> bytes, const Sha256& digest) {
    const auto data = QByteArray::fromRawData(reinterpret_cast<const char*>(bytes.data()), int(bytes.size()));
    const auto result = QCryptographicHash::hash(data, QCryptographicHash::Sha256);
    return std::memcmp(result.constData(), digest.data(), digest.size()) == 0;
  };
  auto footer = image.last(footer_bytes);
  require(digest_matches(image.first(0x108000), expected.boot_area), "NAND boot area differs from the verified seed");
  require(digest_matches(footer, expected.original_footer), "NAND key footer differs from the verified seed");
  detail::ensure_qcore_application();
  const auto layout = inspect::validate_layout(image, true);
  nand_buffer::Device device{std::span<const std::uint8_t>(image)};
  NandBin nand;
  require(nand.SetDevice(&device) && nand.InitNand() && nand.CheckHmacMeta(layout.active_metadata),
          "Cannot authenticate the final NAND filesystem");
  inspect::verify_filesystem(nand, true, layout.active_metadata);

  const QByteArray bytes(reinterpret_cast<const char*>(footer.data()), footer_bytes);
  if (bytes.startsWith("nanddumper@IOS by thepikachugamer\n")) {
    const auto updated = normalize(bytes);
    std::memcpy(footer.data(), updated.constData(), footer_bytes);
  }
  return {footer.begin(), footer.end()};
}

}
