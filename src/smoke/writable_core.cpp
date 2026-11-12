// SPDX-License-Identifier: GPL-2.0-only
#include <wiiform/core.hpp>

#include <nandbin.h>
#include <tools.h>

#include <QByteArray>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QTemporaryDir>
#include <QTreeWidgetItem>
#include <memory>
#include <stdexcept>
#include <string>
#include <unistd.h>

namespace {
  constexpr auto marker_path = "/wiiform/mark";
  constexpr auto marker_data = "wiiform-native-smoke-v1";

  void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }

  auto qpath(std::string_view path) -> QString {
    require(!path.empty(), "NAND destination path is empty");
    return QString::fromUtf8(path.data(), static_cast<qsizetype>(path.size()));
  }
}

namespace wiiform::core {
  void write_synthetic_nand(std::string_view path) {
    const QFileInfo destination(qpath(path));
    require(!destination.exists() && !destination.isSymLink(),
            "NAND destination already exists or is a symlink");
    require(destination.absoluteDir().exists(), "NAND destination parent directory does not exist");

    QTemporaryDir staging(destination.absoluteDir().filePath(".wiiform-native-smoke-XXXXXX"));
    require(staging.isValid(), "Cannot create private staging directory");
    const auto staged_image = staging.filePath("synthetic.nand.bin");
    NandBin nand;
    require(nand.CreateNew(staged_image, QByteArray(0x400, '\0'), QByteArray(0x108000, '\xff'), {42}),
            "WiiQt could not create synthetic NAND");
    require(nand.CreateEntry("/wiiform", 0, 0, NAND_DIR, NAND_RW, NAND_RW, 0),
            "WiiQt could not create marker directory");
    const auto marker = nand.CreateEntry(marker_path, 0, 0, NAND_FILE, NAND_READ, NAND_READ, NAND_READ);
    require(marker && nand.SetData(marker, QByteArray(marker_data)) && nand.WriteMetaData(),
            "WiiQt could not write synthetic NAND marker");

    QFile staged(staged_image);
    require(staged.open(QIODevice::ReadOnly) && fsync(staged.handle()) == 0,
            "Cannot sync synthetic NAND");
    // Hard-link publication is atomic and fails if the destination appeared meanwhile.
    require(link(QFile::encodeName(staged_image).constData(),
                 QFile::encodeName(destination.absoluteFilePath()).constData()) == 0,
            "Cannot publish synthetic NAND (destination must be new)");
  }
} // namespace wiiform::core
