// SPDX-License-Identifier: GPL-2.0-only
#include <BootDiagnostics.hpp>
#include <BuildOptions.hpp>
#include <NandBuffer.hpp>
#include <Titles.hpp>

#include <stdexcept>

namespace {
  void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
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
  }

} // namespace nand_build
