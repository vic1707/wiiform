// SPDX-License-Identifier: GPL-2.0-only
#include <wiiform/write.hpp>

#include <QCoreApplication>
#include <QCryptographicHash>
#include <QFile>
#include <QTemporaryDir>

#include <cstring>
#include <iostream>
#include <stdexcept>

namespace {
  void check(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }

  constexpr qint64 image_size = 0x21000400;
  constexpr qint64 footer_offset = 0x21000000;

  auto sha256(const QByteArray& value) -> std::string {
    return QCryptographicHash::hash(value, QCryptographicHash::Sha256).toHex().toStdString();
  }

  auto fixture_footer() -> QByteArray {
    QByteArray footer(0x400, '\0');
    const QByteArray label = "nanddumper@IOS by thepikachugamer\nConsole ID: 1234ABCD\n";
    std::memcpy(footer.data(), label.constData(), size_t(label.size()));
    footer.replace(0x124, 4, QByteArray::fromHex("1234abcd"));
    for (int i = 0x100; i < footer.size(); ++i)
      if (footer[i] == '\0') footer[i] = char(i & 0xff);
    // Preserve the fixed header bytes and OTP ID in a deterministic fixture.
    std::memcpy(footer.data(), label.constData(), size_t(label.size()));
    footer.replace(0x124, 4, QByteArray::fromHex("1234abcd"));
    return footer;
  }

  auto read_footer(const QString& image) -> QByteArray {
    QFile file(image);
    if (!file.open(QIODevice::ReadOnly) || !file.seek(footer_offset))
      throw std::runtime_error("cannot read fixture footer");
    return file.read(0x400);
  }
}

int main(int argc, char** argv) {
  QCoreApplication app(argc, argv);
  QTemporaryDir dir("build/backupmii-footer-XXXXXX");
  check(dir.isValid(), "BackupMii fixture check failed");
  const auto image = dir.filePath("synthetic-nand.bin");
  const auto keys = dir.filePath("keys.bin");
  const auto original = fixture_footer();

  {
    QFile file(image);
    check(file.open(QIODevice::WriteOnly | QIODevice::NewOnly), "BackupMii fixture check failed");
    check(file.resize(image_size), "BackupMii fixture check failed");
    check(file.seek(footer_offset), "BackupMii fixture check failed");
    check(file.write(original) == original.size(), "BackupMii fixture check failed");
    check(file.flush(), "BackupMii fixture check failed");
  }

  bool rejected = false;
  try {
    (void)wiiform::core::normalize_backupmii_footer(image.toStdString(), std::string(64, '0'));
  } catch (const std::exception&) {
    rejected = true;
  }
  check(rejected, "BackupMii fixture check failed");
  check(read_footer(image) == original, "BackupMii fixture check failed");

  const auto normalized_sha = wiiform::core::normalize_backupmii_footer(image.toStdString(), sha256(original));
  check(!normalized_sha.empty(), "BackupMii fixture check failed");
  const auto normalized = read_footer(image);
  check(normalized.startsWith("BackupMii v1, ConsoleID: 1234abcd\n"), "BackupMii fixture check failed");
  check(sha256(normalized) == normalized_sha, "BackupMii fixture check failed");
  check(normalized.mid(0x100) == original.mid(0x100), "BackupMii fixture check failed");
  check(wiiform::core::normalize_backupmii_footer(image.toStdString(), normalized_sha).empty(), "BackupMii fixture check failed");

  const auto sidecar_sha = wiiform::core::prepare_backupmii_keys(image.toStdString(), keys.toStdString(), normalized_sha);
  check(sidecar_sha == normalized_sha, "BackupMii fixture check failed");
  QFile sidecar(keys);
  check(sidecar.open(QIODevice::ReadOnly), "BackupMii fixture check failed");
  check(sidecar.readAll() == normalized, "BackupMii fixture check failed");

  std::cout << "BackupMii footer fixture passed\n";
}
