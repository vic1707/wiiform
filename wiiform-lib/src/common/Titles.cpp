// SPDX-License-Identifier: GPL-2.0-only
#include <Titles.hpp>
#include <nandinstall.h>

bool initialize_system(NandBin& nand, const QByteArray& uid_data) {
  UIDmap uid(uid_data);
  if (uid_data.isEmpty() || !uid.Check() || !NandCreateDefaultEntries(nand) ||
      !nand.CreateEntry("/shared2/sys", 0, 0, NAND_DIR, NAND_RW, NAND_RW, NAND_RW)) return false;
  const auto entry = nand.CreateEntry("/sys/uid.sys", 0, 0, NAND_FILE, NAND_RW, NAND_RW, 0);
  return entry && nand.SetData(entry, uid_data) && nand.WriteMetaData();
}

bool install_title(NandBin& nand, UIDmap& uid, SharedContentMap& shared,
                   const ValidatedTitleJob& validated) {
  if (!shared.Data().isEmpty() && !shared.Check()) return false;
  return NandInstallTitle(nand, uid, shared, validated.get());
}
