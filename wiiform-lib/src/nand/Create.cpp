// SPDX-License-Identifier: GPL-2.0-only
#include <BootDiagnostics.hpp>
#include <BuildOptions.hpp>
#include <NandBuffer.hpp>
#include <Titles.hpp>

#include <QTreeWidgetItem>
#include <memory>
#include <stdexcept>

namespace {
  void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }

  void verify_created_image(NandBin& nand, const QByteArray& settings, const QByteArray& uids) {
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
  void populate(NandBin& nand, const QByteArray& settings, const QByteArray& uids) {
    require(initialize_system(nand, uids), "Cannot initialize system files");

    const QString menu = "/title/00000001/00000002";
    require(nand.CreateEntry("/title/00000001", 0, 0, NAND_DIR, NAND_RW, NAND_RW, NAND_READ) &&
                nand.CreateEntry(menu, 0, 0, NAND_DIR, NAND_RW, NAND_RW, NAND_READ) &&
                nand.CreateEntry(menu + "/data", 0x1000, 1, NAND_DIR, NAND_RW, 0, 0),
            "Cannot initialize System Menu settings directory");
    const auto setting = nand.CreateEntry(menu + "/data/setting.txt", 0x1000, 1,
                                          NAND_FILE, NAND_READ, NAND_READ, NAND_READ);
    require(setting && nand.SetData(setting, settings) && nand.WriteMetaData(), "Cannot write settings");
  }

  void validate_boot(const QByteArray& boots, inspect::BootPolicy policy) {
    const auto map_blocks = inspect::verify_boot(boots, policy);
    inspect::verify_boot_ecc(boots, policy, map_blocks);
  }
}

namespace nand_build {
  void initialize_image(std::span<std::uint8_t> image, const QByteArray& keys, const QByteArray& boots,
                        const Profile& profile, inspect::BootPolicy policy) {
    require(image.size() == 0x21000400 && keys.size() == 0x400 && boots.size() == 0x108000,
            "Invalid NAND image, footer, or boot area size");
    validate_boot(boots, policy);
    UIDmap uid;
    {
      nand_buffer::Device device(image);
      NandBin nand;
      require(nand.CreateNew(&device, keys, boots, profile.bad_blocks), "Cannot create new NAND");
      populate(nand, profile.settings, uid.Data());
    }
    nand_buffer::Device device{std::span<const std::uint8_t>(image)};
    require(device.read(0x108000) == boots && device.seek(0x21000000) &&
                device.read(0x400) == keys, "Image boot/key readback failed");
    NandBin reopened;
    require(reopened.SetDevice(&device) && reopened.InitNand(), "Cannot reopen generated filesystem");
    verify_created_image(reopened, profile.settings, uid.Data());
  }

} // namespace nand_build
