// SPDX-License-Identifier: GPL-2.0-only
#include <Factory.hpp>
#include <Install.hpp>
#include <Titles.hpp>
#include <tiktmd.h>

#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDir>
#include <QTemporaryDir>
#include <QTreeWidgetItem>
#include <functional>
#include <iostream>
#include <memory>
#include <stdexcept>
#include <sys/file.h>
#include <unistd.h>

namespace {
  struct Content {
    QByteArray bytes;
    quint16 type = 1;
    quint16 index = 0;
  };

  void check(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }

  void rejects(const std::function<void()>& operation, const char* message) {
    try { operation(); }
    catch (const std::exception& error) {
      check(std::string(error.what()).find(message) != std::string::npos, error.what());
      return;
    }
    throw std::runtime_error("Unsafe title install was accepted");
  }

  auto make_job(quint64 tid, quint16 version, const QList<Content>& contents) -> NusJob {
    QByteArray tmd(0x1e4 + contents.size() * 0x24, '\0');
    qToBigEndian<quint32>(ES_SIG_RSA2048, reinterpret_cast<uchar*>(tmd.data()));
    qToBigEndian<quint64>(tid, reinterpret_cast<uchar*>(tmd.data() + 0x18c));
    qToBigEndian<quint16>(version, reinterpret_cast<uchar*>(tmd.data() + 0x1dc));
    qToBigEndian<quint16>(contents.size(), reinterpret_cast<uchar*>(tmd.data() + 0x1de));
    for (int i = 0; i < contents.size(); ++i) {
      auto* record = reinterpret_cast<uchar*>(tmd.data() + 0x1e4 + i * 0x24);
      qToBigEndian<quint32>(static_cast<quint32>(i), record);
      qToBigEndian<quint16>(contents[i].index, record + 4);
      qToBigEndian<quint16>(contents[i].type, record + 6);
      qToBigEndian<quint64>(static_cast<quint64>(contents[i].bytes.size()), record + 8);
      tmd.replace(0x1f4 + i * 0x24, 20, GetSha1(contents[i].bytes));
    }
    QByteArray ticket(0x2a4, '\0');
    qToBigEndian<quint32>(ES_SIG_RSA2048, reinterpret_cast<uchar*>(ticket.data()));
    qToBigEndian<quint64>(tid, reinterpret_cast<uchar*>(ticket.data() + 0x1dc));
    NusJob job;
    job.tid = tid;
    job.version = version;
    job.decrypt = true;
    job.data << tmd << ticket;
    for (const auto& content : contents) job.data << content.bytes;
    return job;
  }

  auto has_item(NandBin& nand, const QString& path) -> bool {
    std::unique_ptr<QTreeWidgetItem> tree(nand.GetTree());
    if (!tree || tree->childCount() != 1 || !path.startsWith('/')) return false;
    auto* item = tree->child(0);
    for (const auto& part : path.split('/', Qt::SkipEmptyParts)) {
      QTreeWidgetItem* found = nullptr;
      for (int i = 0; i < item->childCount(); ++i)
        if (item->child(i)->text(0) == part) { found = item->child(i); break; }
      if (!found) return false;
      item = found;
    }
    return true;
  }

  auto fst_entries(QTreeWidgetItem* item) -> int {
    int result = 1;
    for (int i = 0; i < item->childCount(); ++i) result += fst_entries(item->child(i));
    return result;
  }

  void create_image(const QString& path, const QList<quint16>& bad_blocks = {}) {
    NandBin nand;
    UIDmap uid;
    check(nand.CreateNew(path, QByteArray(0x400, '\0'), QByteArray(0x108000, '\xff'), bad_blocks) &&
              initialize_system(nand, uid.Data()), "Cannot create install test NAND");
    check(nand.WriteMetaData(), "Cannot finalize install test NAND");
  }

  auto digest(const QString& path) -> QByteArray {
    QFile file(path);
    QCryptographicHash hash(QCryptographicHash::Sha256);
    check(file.open(QIODevice::ReadOnly) && hash.addData(&file), "Cannot hash install test NAND");
    return hash.result();
  }

  auto title_path(quint64 tid) -> QString {
    const auto text = QString("%1").arg(tid, 16, 16, QChar('0'));
    return "/title/" + text.left(8) + "/" + text.mid(8);
  }
}

int main(int argc, char** argv) {
  QCoreApplication app(argc, argv);
  try {
    QTemporaryDir directory(QDir::current().filePath("build/install-tests-XXXXXX"));
    check(directory.isValid(), "No test directory");
    const auto image = directory.filePath("install.bin");
    create_image(image);

    const QByteArray private_one("first private payload");
    const QByteArray shared_payload("shared payload reused after reopening");
    const QByteArray private_two("second private payload");
    const quint64 first_tid = 0x100000003ull;
    const quint64 second_tid = 0x100000004ull;
    const quint64 menu_tid = 0x100000002ull;
    auto first = make_job(first_tid, 1, {{private_one, 1, 7}, {shared_payload, 0x8001, 11}});
    auto second = make_job(second_tid, 2, {{private_two, 1, 5}, {shared_payload, 0x8001, 12}});

    const auto offline_cache = directory.filePath("missing-cache");
    const auto initial_digest = digest(image);
    rejects([&] { title_install::install_nus_title(image, offline_cache, first_tid, 1, true); },
            "Offline mode: required title content missing from cache");
    const auto corrupt_title = QDir(offline_cache).filePath("00000001/00000003/v1");
    check(QDir().mkpath(corrupt_title), "Cannot create corrupt title cache fixture");
    for (const auto* name : {"tmd.1", "cetk"}) {
      QFile file(QDir(corrupt_title).filePath(name));
      check(file.open(QIODevice::WriteOnly | QIODevice::NewOnly) && file.write("bad", 3) == 3,
            "Cannot write corrupt cache fixture");
    }
    rejects([&] { title_install::install_nus_title(image, offline_cache, first_tid, 1, true); },
            "Malformed title metadata");
    auto corrupt_job = first;
    corrupt_job.data[2][0] = 'X';
    rejects([&] { title_install::detail::install_validated_job(image, corrupt_job); }, "content hash validation failed");
    check(digest(image) == initial_digest, "Invalid cache or title data changed the target NAND");

    const auto alias = directory.filePath("alias.bin");
    check(link(QFile::encodeName(image).constData(), QFile::encodeName(alias).constData()) == 0,
          "Cannot create a hard link to the test NAND");
    {
      QFile lock(image);
      check(lock.open(QIODevice::ReadWrite) && flock(lock.handle(), LOCK_EX | LOCK_NB) == 0,
            "Cannot lock the test NAND");
      rejects([&] { title_install::detail::install_validated_job(alias, first); }, "already holds the NAND image lock");
      check(digest(image) == initial_digest, "A concurrent install changed the locked NAND");
    }

    title_install::detail::install_validated_job(image, first);
    {
      NandBin nand;
      check(nand.SetPath(image) && nand.InitNand(), "Cannot reopen first installed image");
      check(nand.GetData(title_path(first_tid) + "/content/00000000.app") == private_one,
            "Private title content did not persist");
      SharedContentMap map(nand.GetData("/shared1/content.map"));
      check(map.Check() && map.Count() == 1 && map.GetAppFromHash(GetSha1(shared_payload)) == "00000000" &&
                nand.GetData("/shared1/00000000.app") == shared_payload,
            "Shared content map did not persist");
    }

    // A second invocation starts from a reopened NAND and must re-use the
    // indexed shared app while assigning and persisting a new UID.
    title_install::detail::install_validated_job(image, second);
    {
      NandBin nand;
      check(nand.SetPath(image) && nand.InitNand(), "Cannot reopen second installed image");
      SharedContentMap map(nand.GetData("/shared1/content.map"));
      UIDmap uid(nand.GetData("/sys/uid.sys"));
      check(map.Check() && map.Count() == 1 && uid.Check() && uid.GetUid(first_tid, false) == 0x1001 &&
                uid.GetUid(second_tid, false) == 0x1002,
            "Repeated install did not persist shared-map/UID state");
      check(nand.GetData(title_path(second_tid) + "/content/00000000.app") == private_two,
            "Second title content did not persist");
    }

    // The fresh builder pre-creates only this system-menu data skeleton.
    // Installation must preserve settings while adding the stock TMD/ticket.
    const QByteArray settings("preserve-me");
    {
      NandBin nand;
      check(nand.SetPath(image) && nand.InitNand(), "Cannot reopen image for menu skeleton");
      for (const auto& [path, attr] : QList<QPair<QString, quint8>>{
             {"/title/00000001", NAND_DIR}, {"/title/00000001/00000002", NAND_DIR},
             {"/title/00000001/00000002/data", NAND_DIR}}) {
        if (!has_item(nand, path))
          check(nand.CreateEntry(path, 0, 1, attr, NAND_RW, NAND_RW, 0), "Cannot create menu skeleton");
      }
      const auto setting_path = title_path(menu_tid) + "/data/setting.txt";
      if (!has_item(nand, setting_path)) {
        const auto entry = nand.CreateEntry(setting_path, 0x1000, 1, NAND_FILE, NAND_READ, NAND_READ, NAND_READ);
        check(entry && nand.SetData(entry, settings), "Cannot create menu settings fixture");
      }
      check(nand.WriteMetaData(), "Cannot persist menu skeleton");
    }
    auto menu = make_job(menu_tid, 514, {{QByteArray("menu content"), 1, 9}});
    title_install::detail::install_validated_job(image, menu);
    {
      NandBin nand;
      check(nand.SetPath(image) && nand.InitNand() && nand.GetData(title_path(menu_tid) + "/data/setting.txt") == settings &&
                !nand.GetData(title_path(menu_tid) + "/content/title.tmd").isEmpty(),
            "System-menu install did not preserve the pre-created settings file");
    }

    rejects([&] { title_install::detail::install_validated_job(image, first); }, "already installed");
    {
      NandBin nand;
      check(nand.SetPath(image) && nand.InitNand() &&
                nand.GetData(title_path(first_tid) + "/content/00000000.app") == private_one,
            "Duplicate title install changed existing content");
    }

    // Fill the FST but leave too few records for a new title. Capacity checks
    // must run before the first package write.
    {
      NandBin nand;
      check(nand.SetPath(image) && nand.InitNand(), "Cannot reopen image for FST-capacity test");
      std::unique_ptr<QTreeWidgetItem> tree(nand.GetTree());
      const int leave_slots = 2;
      int used = fst_entries(tree->child(0));
      quint32 group = 0;
      while (used < 0x17ff - leave_slots) {
        const auto directory = "/tmp/d" + QString::number(group++);
        check(nand.CreateEntry(directory, 0, 0, NAND_DIR, NAND_RW, NAND_RW, 0), "Cannot group FST fixture entries");
        ++used;
        for (int file = 0; file < 100 && used < 0x17ff - leave_slots; ++file) {
          check(nand.CreateEntry(directory + "/f" + QString::number(file), 0, 0, NAND_FILE, NAND_RW, NAND_RW, 0),
                "Cannot fill FST fixture");
          ++used;
        }
      }
      check(nand.WriteMetaData(), "Cannot persist full FST fixture");
    }
    const auto third_tid = 0x100000005ull;
    auto third = make_job(third_tid, 1, {{QByteArray("not written"), 1, 1}});
    const auto full_fst_digest = digest(image);
    rejects([&] { title_install::detail::install_validated_job(image, third); }, "Insufficient free NAND FST entries");
    check(digest(image) == full_fst_digest, "FST-capacity failure mutated the NAND");

    const auto nearly_full = directory.filePath("nearly-full.bin");
    QList<quint16> bad_blocks;
    for (quint16 block = 9; block < 4080; ++block) bad_blocks.append(block);
    create_image(nearly_full, bad_blocks);
    int free_clusters = 0;
    {
      NandBin nand;
      check(nand.SetPath(nearly_full) && nand.InitNand(), "Cannot reopen nearly full fixture");
      free_clusters = nand.GetFats().count(0xfffe);
    }
    check(free_clusters > 2, "Fixture cannot accommodate ticket, TMD and content");
    const auto exact_fit = make_job(third_tid, 1, {{QByteArray((free_clusters - 2) * 0x4000, 'p'), 1, 1}});
    const auto nearly_full_digest = digest(nearly_full);
    rejects([&] { title_install::detail::install_validated_job(nearly_full, exact_fit); }, "one cluster is reserved");
    check(digest(nearly_full) == nearly_full_digest, "Cluster-capacity failure mutated the NAND");

    std::cout << "install tests passed\n";
    return 0;
  } catch (const std::exception& error) {
    std::cerr << "install tests failed: " << error.what() << '\n';
    return 1;
  }
}
