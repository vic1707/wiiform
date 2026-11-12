// SPDX-License-Identifier: GPL-2.0-only
// Adapted from giantpune's WiiQt ohneschwanzenegger title handling code.
#include <Titles.hpp>
#include <tiktmd.h>

bool check_title_metadata(const QByteArray& tmd, const QByteArray& ticket) {
  // Validate signed headers before the legacy Tmd/Ticket constructors read them.
  // shortcut: Wii titles use RSA-2048; validate other signature layouts before supporting them.
  if (tmd.size() < 0x1e4 || ticket.size() < 0x2a4 ||
      qFromBigEndian<quint32>(reinterpret_cast<const uchar*>(tmd.constData())) != ES_SIG_RSA2048 ||
      qFromBigEndian<quint32>(reinterpret_cast<const uchar*>(ticket.constData())) != ES_SIG_RSA2048 ||
      !tmd.mid(0x140, 0x40).contains('\0') || !ticket.mid(0x140, 0x40).contains('\0')) return false;
  const auto count = qFromBigEndian<quint16>(reinterpret_cast<const uchar*>(tmd.constData() + 0x1de));
  return count && tmd.size() >= 0x1e4 + count * 0x24;
}

bool decrypt_title(NusJob& job) {
  if (!job.tid || job.data.size() < 3 || !check_title_metadata(job.data[0], job.data[1])) return false;
  const auto count = qFromBigEndian<quint16>(reinterpret_cast<const uchar*>(job.data[0].constData() + 0x1de));
  if (job.data.size() != count + 2) return false;
  Tmd tmd(job.data[0]);
  Ticket ticket(job.data[1], false);
  if (tmd.Tid() != job.tid || ticket.Tid() != job.tid || tmd.Version() != job.version ||
      static_cast<uchar>(job.data[1][0x1f1]) != 0) return false;

  // Check every content before changing the filesystem; hashes are not signature verification.
  QSet<quint16> indices;
  QSet<QString> cids;
  quint64 total_size = 0;
  for (int i = 0; i < count; ++i) {
    const auto size = tmd.Size(i);
    total_size += size;
    if (size > 0x20000000 || total_size > 0x20000000 || indices.contains(tmd.Index(i)) ||
        cids.contains(tmd.Cid(i)) || (tmd.Type(i) != 1 && tmd.Type(i) != 0x8001)) return false;
    indices.insert(tmd.Index(i));
    cids.insert(tmd.Cid(i));
    if (!job.decrypt) {
      if (job.data[i + 2].size() % 16 || quint64(job.data[i + 2].size()) < ((size + 15) & ~quint64(15))) return false;
      AesSetKey(ticket.DecryptedKey());
      job.data[i + 2] = AesDecrypt(tmd.Index(i), job.data[i + 2]);
    }
    if (quint64(job.data[i + 2].size()) < size) return false;
    job.data[i + 2].resize(int(size));
    if (GetSha1(job.data[i + 2]) != tmd.Hash(i)) return false;
  }
  job.decrypt = true;
  return true;
}

