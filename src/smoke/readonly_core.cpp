// SPDX-License-Identifier: GPL-2.0-only
#include <wiiform/core.hpp>

#include <nandbin.h>

#include <QByteArray>
#include <QString>
#include <QTreeWidgetItem>
#include <memory>
#include <stdexcept>

namespace {
  constexpr auto marker_path = "/wiiform/mark";
  constexpr auto expected_marker = "wiiform-native-smoke-v1";

  void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }
}

namespace wiiform::core {
  auto inspect_synthetic_nand(std::string_view path) -> std::string {
    require(!path.empty(), "NAND image path is empty");
    const auto image_path = QString::fromUtf8(path.data(), static_cast<qsizetype>(path.size()));
    NandBin nand;
    require(nand.SetPath(image_path) && nand.InitNand(), "WiiQt could not read NAND filesystem");
    std::unique_ptr<QTreeWidgetItem> tree(nand.GetTree());
    require(tree && tree->childCount() == 1, "Unexpected NAND filesystem tree");
    const auto marker = nand.GetData(marker_path);
    require(marker == QByteArray(expected_marker), "NAND smoke-test marker is absent or invalid");
    return marker.toStdString();
  }
} // namespace wiiform::core
