// SPDX-License-Identifier: GPL-2.0-only
#include <wiiform/write.hpp>
#include "QtRuntime.hpp"

#include <QCryptographicHash>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QRegularExpression>

#include <sys/file.h>
#include <unistd.h>

#include <stdexcept>
#include <memory>
#include <utility>

namespace wiiform::core {
namespace {
  constexpr qint64 nand_bytes = 0x21000400;
  constexpr qint64 footer_bytes = 0x400;
  constexpr qint64 footer_header_bytes = 0x100;
  constexpr qint64 footer_offset = 0x21000000;
  constexpr qint64 otp_console_id_offset = 0x124;

  void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }

  auto qstring(const std::string& value, const char* label) -> QString {
    const auto converted = QString::fromUtf8(value.data(), qsizetype(value.size()));
    require(converted.toUtf8() == QByteArray(value.data(), qsizetype(value.size())), label);
    return converted;
  }

  auto sha256(const QByteArray& bytes) -> QByteArray {
    return QCryptographicHash::hash(bytes, QCryptographicHash::Sha256).toHex();
  }

  auto expected_sha256(const std::string& value) -> QByteArray {
    const auto text = qstring(value, "SHA-256 must be ASCII");
    static const QRegularExpression pattern("^[0-9a-f]{64}$");
    require(pattern.match(text).hasMatch(), "SHA-256 must contain 64 lowercase hexadecimal digits");
    return text.toLatin1();
  }

  auto read_authenticated_footer(QFile& file, const std::string& expected) -> QByteArray {
    require(file.isOpen() && file.size() == nand_bytes,
            "Expected a regular generated NAND image with its BootMii key footer");
    require(file.seek(footer_offset), "Cannot seek to the BootMii key footer");
    const auto bytes = file.read(footer_bytes);
    require(bytes.size() == footer_bytes && sha256(bytes) == expected_sha256(expected),
            "NAND key footer differs from the authenticated inventory");
    return bytes;
  }

  auto open_locked_image(const std::string& image, QIODevice::OpenMode mode) -> std::unique_ptr<QFile> {
    const auto path = qstring(image, "Image path must be UTF-8");
    const QFileInfo info(path);
    auto file = std::make_unique<QFile>(path);
    require(info.isFile() && !info.isSymLink() && file->open(mode),
            "Expected a regular NAND image file");
    const auto lock = mode.testFlag(QIODevice::WriteOnly) ? LOCK_EX : LOCK_SH;
    require(flock(file->handle(), lock | LOCK_NB) == 0, "Cannot lock the NAND image");
    return file;
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

std::string normalize_backupmii_footer(const std::string& image, const std::string& expected) {
  detail::ensure_qcore_application();
  auto file = open_locked_image(image, QIODevice::ReadWrite);
  const auto footer = read_authenticated_footer(*file, expected);
  constexpr char legacy[] = "nanddumper@IOS by thepikachugamer\n";
  if (!footer.startsWith(QByteArray(legacy, int(sizeof(legacy) - 1)))) return {};
  const auto updated = normalize(footer);
  require(file->seek(footer_offset) && file->write(updated) == updated.size() && file->flush() &&
              fsync(file->handle()) == 0,
          "Cannot normalize the BackupMii footer");
  return sha256(updated).toStdString();
}

std::string prepare_backupmii_keys(const std::string& image, const std::string& target,
                                   const std::string& expected) {
  detail::ensure_qcore_application();
  auto file = open_locked_image(image, QIODevice::ReadOnly);
  const auto keys = read_authenticated_footer(*file, expected);
  const auto path = qstring(target, "Keys output path must be UTF-8");
  const QFileInfo destination(path);
  require(!destination.exists() && !destination.isSymLink() && destination.absoluteDir().exists(),
          "Keys output requires a new path in an existing directory");
  QFile output(path);
  require(output.open(QIODevice::WriteOnly | QIODevice::NewOnly) &&
              output.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner),
          "Cannot create the private keys sidecar");
  require(output.write(keys) == footer_bytes && output.flush() && fsync(output.handle()) == 0,
          "Cannot write the private keys sidecar");
  return sha256(keys).toStdString();
}
}
