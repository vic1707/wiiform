// SPDX-License-Identifier: GPL-2.0-only
// Adapted from giantpune's WiiQt ohneschwanzenegger title installation code.
#include <Titles.hpp>
#include <tiktmd.h>

namespace {
  auto ensure_entry(NandBin& nand, const QString& path, quint32 uid, quint16 gid,
                    quint8 attr, quint8 user, quint8 group, quint8 other) -> quint16 {
    NandEntry existing;
    if (nand.Stat(path, &existing)) return existing.index;
    return nand.CreateEntry(path, uid, gid, attr, user, group, other);
  }
}

bool initialize_system(NandBin& nand, const QByteArray& uid_data) {
  UIDmap uid(uid_data);
  if (uid_data.isEmpty() || !uid.Check()) return false;
  if (!nand.CreateEntry("/sys", 0, 0, NAND_DIR, NAND_RW, NAND_RW, 0) ||
      !nand.CreateEntry("/ticket", 0, 0, NAND_DIR, NAND_RW, NAND_RW, 0) ||
      !nand.CreateEntry("/title", 0, 0, NAND_DIR, NAND_RW, NAND_RW, NAND_READ) ||
      !nand.CreateEntry("/shared1", 0, 0, NAND_DIR, NAND_RW, NAND_RW, 0) ||
      !nand.CreateEntry("/shared2", 0, 0, NAND_DIR, NAND_RW, NAND_RW, NAND_RW) ||
      !nand.CreateEntry("/shared2/sys", 0, 0, NAND_DIR, NAND_RW, NAND_RW, NAND_RW) ||
      !nand.CreateEntry("/import", 0, 0, NAND_DIR, NAND_RW, NAND_RW, 0) ||
      !nand.CreateEntry("/meta", 0x1000, 1, NAND_DIR, NAND_RW, NAND_RW, NAND_RW) ||
      !nand.CreateEntry("/tmp", 0, 0, NAND_DIR, NAND_RW, NAND_RW, NAND_RW)) return false;
  const auto cert = nand.CreateEntry("/sys/cert.sys", 0, 0, NAND_FILE, NAND_RW, NAND_RW, NAND_READ);
  const auto entry = nand.CreateEntry("/sys/uid.sys", 0, 0, NAND_FILE, NAND_RW, NAND_RW, 0);
  return cert && entry && nand.SetData(cert, QByteArray(reinterpret_cast<const char*>(certs_dat), CERTS_DAT_SIZE)) &&
         nand.SetData(entry, uid_data) && nand.WriteMetaData();
}

bool install_title(NandBin& nand, UIDmap& uid, SharedContentMap& shared,
                   const ValidatedTitleJob& validated) {
  if (!shared.Data().isEmpty() && !shared.Check()) return false;
  const auto& job = validated.get();
  Tmd tmd(job.data[0]);
  Ticket ticket(job.data[1], false);
  const auto count = tmd.Count();

  const auto tid = QString("%1").arg(job.tid, 16, 16, QChar('0'));
  const auto title = "/title/" + tid.left(8) + "/" + tid.mid(8);
  const auto ticket_path = "/ticket/" + tid.left(8);
  const auto owner = uid.GetUid(job.tid);
  if (!owner || !ensure_entry(nand, ticket_path, 0, 0, NAND_DIR, NAND_RW, NAND_RW, 0) ||
      !ensure_entry(nand, "/title/" + tid.left(8), 0, 0, NAND_DIR, NAND_RW, NAND_RW, NAND_READ) ||
      !ensure_entry(nand, title, 0, 0, NAND_DIR, NAND_RW, NAND_RW, NAND_READ) ||
      !ensure_entry(nand, title + "/data", owner, tmd.Gid(), NAND_DIR, NAND_RW, 0, 0) ||
      !ensure_entry(nand, title + "/content", 0, 0, NAND_DIR, NAND_RW, NAND_RW, 0)) return false;

  NandEntry content;
  if (!nand.Stat(title + "/content", &content)) return false;
  for (const auto& child : nand.Children(content.index))
    if (!nand.Delete(title + "/content/" + child.name)) return false;

  auto write = [&](const QString& path, const QByteArray& data) {
    const auto entry = ensure_entry(nand, path, 0, 0, NAND_FILE, NAND_RW, NAND_RW, 0);
    return entry && nand.SetData(entry, data);
  };
  if (!write(ticket_path + "/" + tid.mid(8) + ".tik", ticket.Data()) ||
      !write(title + "/content/title.tmd", tmd.Data())) return false;
  for (int i = 0; i < count; ++i) {
    auto path = title + "/content/" + tmd.Cid(i) + ".app";
    if (tmd.Type(i) == 0x8001) {
      if (!shared.GetAppFromHash(tmd.Hash(i)).isEmpty()) continue;
      const auto cid = shared.GetNextEmptyCid();
      path = "/shared1/" + cid + ".app";
      if (!write(path, job.data[i + 2])) return false;
      shared.AddEntry(cid, tmd.Hash(i));
    } else if (!write(path, job.data[i + 2])) return false;
  }
  return true;
}
