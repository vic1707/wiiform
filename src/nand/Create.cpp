// SPDX-License-Identifier: GPL-2.0-only
#include <BootDiagnostics.hpp>
#include <BuildOptions.hpp>
#include <Titles.hpp>

#include <QCryptographicHash>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QTemporaryDir>
#include <QTreeWidgetItem>
#include <memory>
#include <stdexcept>

namespace {
  void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }

  void verify_created_image(const QString& path, const QByteArray& keys, const QByteArray& boots,
                            const QByteArray& settings, const QByteArray& uids) {
    QFile file(path);
    require(file.open(QIODevice::ReadOnly) && file.size() == 0x21000400 && file.read(0x108000) == boots &&
                file.seek(0x21000000) && file.read(0x400) == keys, "Image boot/key readback failed");
    NandBin nand;
    require(nand.SetPath(path) && nand.InitNand(), "Cannot reopen generated filesystem");
    require(nand.GetData("/title/00000001/00000002/data/setting.txt") == settings &&
                nand.GetData("/sys/uid.sys") == uids, "Settings/UID readback failed");
    for (int cluster = 0x7f00; cluster < 0x8000; cluster += 16)
      require(nand.CheckHmacMeta(cluster), "Generated metadata HMAC is invalid");
    const auto fats = nand.GetFats();
    for (int cluster = 0x40; cluster < 0x8000; ++cluster) {
      if (fats[cluster] == 0xfffd || fats[cluster] == 0xfffe) continue;
      for (int page = 0; page < 8; ++page)
        require(nand.CheckEcc(cluster * 8 + page), "Generated filesystem ECC is invalid");
    }
    std::unique_ptr<QTreeWidgetItem> tree(nand.GetTree());
    auto verify = [&](auto&& self, QTreeWidgetItem* item) -> void {
      if (item->text(7).left(2).toInt(nullptr, 16) == NAND_FILE)
        require(nand.CheckHmacData(item->text(1).toUShort()), "Generated file HMAC is invalid");
      for (int i = 0; i < item->childCount(); ++i) self(self, item->child(i));
    };
    verify(verify, tree->child(0));
    SharedContentMap shared(nand.GetData("/shared1/content.map"));
    require(shared.Data().isEmpty() || shared.Check(), "Generated shared-content map is invalid");
  }
}

namespace nand_build {
  void create(const QString& seed_path, const QString& output, const Options& options) {
    const QFileInfo destination(QDir::cleanPath(QFileInfo(output).absoluteFilePath()));
    require(!destination.exists() && !destination.isSymLink(), "Destination already exists");
    require(destination.absoluteDir().exists(), "Destination parent must exist");
    const auto components = seed::read(seed_path);
    const auto profile = resolve_profile(components, options);
    const auto keys = components.files.value("keys.bin");
    const auto boots = components.files.value("boot-blocks.bin");
    inspect::verify_boot(boots, options.boot_policy);
    require(!preflight::diagnose_boot_ecc(boots, boots, options.boot_policy).blocked,
            "Boot-page ECC mismatches require review");

    QTemporaryDir staging(destination.absoluteDir().filePath(".wii-nand-XXXXXX"));
    require(staging.isValid(), "Cannot create private staging directory");
    const auto image = staging.filePath("nand.bin");
    UIDmap uid;
    {
      NandBin nand;
      require(nand.CreateNew(image, keys, boots, profile.bad_blocks), "Cannot create new NAND");
      require(QFile::setPermissions(image, QFileDevice::ReadOwner | QFileDevice::WriteOwner), "Cannot protect output");
      require(initialize_system(nand, uid.Data()), "Cannot initialize system files");

      const QString menu = "/title/00000001/00000002";
      require(nand.CreateEntry("/title/00000001", 0, 0, NAND_DIR, NAND_RW, NAND_RW, NAND_READ) &&
                  nand.CreateEntry(menu, 0, 0, NAND_DIR, NAND_RW, NAND_RW, NAND_READ) &&
                  nand.CreateEntry(menu + "/data", 0x1000, 1, NAND_DIR, NAND_RW, 0, 0),
              "Cannot initialize System Menu settings directory");
      const auto setting = nand.CreateEntry(menu + "/data/setting.txt", 0x1000, 1,
                                            NAND_FILE, NAND_READ, NAND_READ, NAND_READ);
      require(setting && nand.SetData(setting, profile.settings) && nand.WriteMetaData(), "Cannot write settings");
    }
    verify_created_image(image, keys, boots, profile.settings, uid.Data());
    require(QFile::rename(image, destination.absoluteFilePath()), "Cannot publish NAND to a new destination");
    qInfo().noquote() << "Boot policy" << boot_policy_name(options.boot_policy) << "with SHA-256"
                     << QCryptographicHash::hash(boots, QCryptographicHash::Sha256).toHex();
  }
} // namespace nand_build
