// SPDX-License-Identifier: GPL-2.0-only
#include <Install.hpp>
#include <Remove.hpp>
#include <Titles.hpp>
#include <tiktmd.h>

#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDir>
#include <QFile>
#include <QTemporaryDir>
#include <QTreeWidgetItem>
#include <functional>
#include <iostream>
#include <memory>
#include <stdexcept>

namespace {
  constexpr quint64 forecast_id = 0x0001000248414645ull;
  constexpr quint64 shop_id = 0x0001000248414241ull;
  constexpr quint64 photo_base_id = 0x0001000248414141ull;
  constexpr quint64 photo_11_id = 0x0001000248415941ull;

  void check(bool ok, const char* message) {
    if (!ok) throw std::runtime_error(message);
  }

  void rejects(const std::function<void()>& operation, const char* message) {
    try {
      operation();
    } catch (const std::exception& error) {
      check(std::string(error.what()).find(message) != std::string::npos, error.what());
      return;
    }
    throw std::runtime_error("Unsafe channel removal was accepted");
  }

  auto make_job(quint64 title_id, const QList<QByteArray>& contents,
                const QList<quint16>& types) -> NusJob {
    QByteArray tmd(0x1e4 + contents.size() * 0x24, '\0');
    qToBigEndian<quint32>(ES_SIG_RSA2048, reinterpret_cast<uchar*>(tmd.data()));
    qToBigEndian<quint64>(title_id, reinterpret_cast<uchar*>(tmd.data() + 0x18c));
    qToBigEndian<quint16>(1, reinterpret_cast<uchar*>(tmd.data() + 0x1dc));
    qToBigEndian<quint16>(contents.size(), reinterpret_cast<uchar*>(tmd.data() + 0x1de));
    for (int i = 0; i < contents.size(); ++i) {
      auto* record = reinterpret_cast<uchar*>(tmd.data() + 0x1e4 + i * 0x24);
      qToBigEndian<quint32>(static_cast<quint32>(i), record);
      qToBigEndian<quint16>(static_cast<quint16>(i), record + 4);
      qToBigEndian<quint16>(types[i], record + 6);
      qToBigEndian<quint64>(static_cast<quint64>(contents[i].size()), record + 8);
      tmd.replace(0x1f4 + i * 0x24, 20, GetSha1(contents[i]));
    }
    QByteArray ticket(0x2a4, '\0');
    qToBigEndian<quint32>(ES_SIG_RSA2048, reinterpret_cast<uchar*>(ticket.data()));
    qToBigEndian<quint64>(title_id, reinterpret_cast<uchar*>(ticket.data() + 0x1dc));
    NusJob job;
    job.tid = title_id;
    job.version = 1;
    job.decrypt = true;
    job.data << tmd << ticket;
    for (const auto& content : contents) job.data << content;
    return job;
  }

  auto digest(const QString& path) -> QByteArray {
    QFile file(path);
    QCryptographicHash hash(QCryptographicHash::Sha256);
    check(file.open(QIODevice::ReadOnly) && hash.addData(&file), "Cannot hash removal fixture");
    return hash.result();
  }

  auto title_path(quint64 title_id) -> QString {
    const auto name = QString("%1").arg(title_id, 16, 16, QChar('0'));
    return "/title/" + name.left(8) + "/" + name.mid(8);
  }

  auto ticket_path(quint64 title_id) -> QString {
    const auto name = QString("%1").arg(title_id, 16, 16, QChar('0'));
    return "/ticket/" + name.left(8) + "/" + name.mid(8) + ".tik";
  }

  void create_image(const QString& path) {
    NandBin nand;
    UIDmap uid;
    check(nand.CreateNew(path, QByteArray(0x400, '\0'), QByteArray(0x108000, '\xff'), {}) &&
              initialize_system(nand, uid.Data()) && nand.WriteMetaData(),
          "Cannot create removal test image");
  }

  void verify_forecast_removal(const QString& image) {
    create_image(image);
    const QByteArray private_content("forecast private content");
    const QByteArray shared_content("shared forecast graphic");

    title_install::detail::install_validated_job(
        image, make_job(forecast_id, {private_content, shared_content}, {1, 0x8001}));
    {
      NandBin nand;
      check(nand.SetPath(image) && nand.InitNand(), "Cannot reopen test NAND after title install");
      UIDmap uid(nand.GetData("/sys/uid.sys"));
      SharedContentMap shared(nand.GetData("/shared1/content.map"));
      check(uid.Check() && uid.GetUid(forecast_id, false) != 0 && shared.Check() &&
                shared.GetAppFromHash(GetSha1(shared_content)) == "00000000",
            "Installed channel did not persist its UID and shared-content records");

      const auto data_file = nand.CreateEntry(title_path(forecast_id) + "/data/save.bin",
                                               uid.GetUid(forecast_id, false), 0, NAND_FILE, NAND_RW, 0, 0);
      check(data_file && nand.SetData(data_file, QByteArray("preserved savedata")) && nand.WriteMetaData(),
            "Cannot write channel save fixture");
    }

    const auto initial_digest = digest(image);
    rejects([&] { title_remove::remove_channel(image, 0x0000000100000002ull); },
            "Only allowlisted Forecast, News, Shop and Photo Channel");
    rejects([&] { title_remove::remove_channel(image, 0x0001000248414341ull); },
            "Only allowlisted Forecast, News, Shop and Photo Channel");
    rejects([&] { title_remove::remove_channel(image, 0x0001000048415a41ull); },
            "Only allowlisted Forecast, News, Shop and Photo Channel");
    check(digest(image) == initial_digest, "Unsupported title ID changed the NAND");
    rejects([&] { title_remove::remove_channel(image, 0x0001000248414641ull); },
            "Allowlisted channel is not completely installed");
    check(digest(image) == initial_digest, "Missing allowlisted title changed the NAND");

    QByteArray uid_before, map_before, shared_before, save_before;
    {
      NandBin nand;
      check(nand.SetPath(image) && nand.InitNand(), "Cannot inspect installed channel before removal");
      uid_before = nand.GetData("/sys/uid.sys");
      map_before = nand.GetData("/shared1/content.map");
      shared_before = nand.GetData("/shared1/00000000.app");
      save_before = nand.GetData(title_path(forecast_id) + "/data/save.bin");
    }
    title_remove::remove_channel(image, forecast_id);
    {
      NandBin nand;
      check(nand.SetPath(image) && nand.InitNand(), "Cannot reopen removed-channel NAND");
      check(nand.GetData("/sys/uid.sys") == uid_before &&
                nand.GetData("/shared1/content.map") == map_before &&
                nand.GetData("/shared1/00000000.app") == shared_before,
            "Channel removal changed UID or shared-content state");
      check(nand.GetData(title_path(forecast_id) + "/data/save.bin") == save_before &&
                nand.GetData(title_path(forecast_id) + "/content/title.tmd").isEmpty() &&
                nand.GetData(ticket_path(forecast_id)).isEmpty(),
            "Channel removal did not preserve data or remove both channel records");
      SharedContentMap map(nand.GetData("/shared1/content.map"));
      UIDmap uid(nand.GetData("/sys/uid.sys"));
      check(map.Check() && uid.Check() && uid.GetUid(forecast_id, false) != 0,
            "Channel removal damaged persistent UID or shared-content maps");
    }
  }

  void verify_combined_shop_photo_removal(const QString& image) {
    create_image(image);
    const QList<quint64> title_ids{shop_id, photo_base_id, photo_11_id};
    const QByteArray shared_content("content shared by Shop and Photo fixtures");
    for (const auto title_id : title_ids) {
      const QByteArray private_content(QString("private %1").arg(title_id, 16, 16, QChar('0')).toUtf8());
      title_install::detail::install_validated_job(
          image, make_job(title_id, {private_content, shared_content}, {1, 0x8001}));
      NandBin nand;
      check(nand.SetPath(image) && nand.InitNand(), "Cannot reopen combined-removal fixture");
      UIDmap uid(nand.GetData("/sys/uid.sys"));
      check(uid.Check() && uid.GetUid(title_id, false) != 0,
            "Combined-removal title has no UID record");
      const auto data_file = nand.CreateEntry(title_path(title_id) + "/data/keep.bin",
                                               uid.GetUid(title_id, false), 0, NAND_FILE, NAND_RW, 0, 0);
      check(data_file && nand.SetData(data_file, QByteArray("kept")) && nand.WriteMetaData(),
            "Cannot prepare combined-removal title data");
    }

    QByteArray map_before, shared_before;
    {
      NandBin nand;
      check(nand.SetPath(image) && nand.InitNand(), "Cannot inspect combined-removal fixture");
      map_before = nand.GetData("/shared1/content.map");
      shared_before = nand.GetData("/shared1/00000000.app");
      check(!map_before.isEmpty() && !shared_before.isEmpty(), "Combined fixture lacks shared content");
    }

    for (const auto title_id : title_ids) {
      title_remove::remove_channel(image, title_id);
      NandBin nand;
      check(nand.SetPath(image) && nand.InitNand(), "Cannot reopen cumulatively cleaned NAND");
      UIDmap uid(nand.GetData("/sys/uid.sys"));
      SharedContentMap shared(nand.GetData("/shared1/content.map"));
      check(nand.GetData(title_path(title_id) + "/data/keep.bin") == QByteArray("kept") &&
                nand.GetData(title_path(title_id) + "/content/title.tmd").isEmpty() &&
                nand.GetData(ticket_path(title_id)).isEmpty() && uid.Check() &&
                uid.GetUid(title_id, false) != 0 && shared.Check() &&
                nand.GetData("/shared1/content.map") == map_before &&
                nand.GetData("/shared1/00000000.app") == shared_before,
            "Combined Shop/Photo removal changed savedata, UIDs or shared content");
    }
  }
}

int main(int argc, char** argv) {
  QCoreApplication app(argc, argv);
  try {
    QTemporaryDir directory(QDir::current().filePath("build/remove-tests-XXXXXX"));
    check(directory.isValid(), "Cannot create removal test directory");
    verify_forecast_removal(directory.filePath("forecast.bin"));
    verify_combined_shop_photo_removal(directory.filePath("shop-photo.bin"));

    std::cout << "Forecast/News/Shop/Photo allowlist, save preservation and map retention passed\n";
    return 0;
  } catch (const std::exception& error) {
    std::cerr << "Removal tests failed: " << error.what() << '\n';
    return 1;
  }
}
