// SPDX-License-Identifier: GPL-2.0-only
#include <Inspection.hpp>

#include <Factory.hpp>
#include <Titles.hpp>
#include <sharedcontentmap.h>
#include <tiktmd.h>
#include <uidmap.h>

#include <QCryptographicHash>
#include <QDir>
#include <QMap>
#include <QSet>
#include <QtEndian>
#include <optional>
#include <stdexcept>

namespace {
  constexpr int maximum_entries = 0x10000;
  constexpr int maximum_depth = 64;
  constexpr quint64 maximum_file_size = 0x21000000;
  constexpr quint64 maximum_title_size = 0x20000000;

  void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }

  auto is_hex_id(const QString& text) -> bool {
    if (text.size() != 8) return false;
    for (const auto c : text)
      if (!((c >= QChar('0') && c <= QChar('9')) || (c >= QChar('a') && c <= QChar('f')) ||
            (c >= QChar('A') && c <= QChar('F')))) return false;
    return true;
  }

  auto title_path(quint64 id) -> QString {
    return QString("%1").arg(id, 16, 16, QChar('0'));
  }

  auto hex_id(const QString& text, bool& ok) -> quint32 {
    if (!is_hex_id(text)) {
      ok = false;
      return 0;
    }
    return text.toUInt(&ok, 16);
  }

  struct Metadata {
    quint32 uid = 0;
    quint16 gid = 0;
    quint32 x3 = 0;
    quint8 mode = 0;
    quint8 attr = 0;
  };

  auto entry_metadata(const NandEntry& entry) -> Metadata {
    return {entry.uid, entry.gid, entry.x3, entry.wtf, entry.attr};
  }

  auto make_attr(int type, int user, int group, int other) -> quint8 {
    return static_cast<quint8>((type & 3) | ((user & 3) << 6) | ((group & 3) << 4) | ((other & 3) << 2));
  }

  struct ExpectedMetadata {
    quint32 uid = 0;
    quint16 gid = 0;
    quint8 attr = 0;
    quint32 x3 = 0;
    quint8 mode = 0;
  };

  struct Entry {
    quint16 id = 0;
    quint64 size = 0;
    bool directory = false;
    Metadata metadata;
  };

  class ImageIndex {
  public:
    explicit ImageIndex(NandBin& nand) : nand_(nand) {
      const auto fats = nand_.GetFats();
      require(fats.size() == 0x8000, "Invalid NAND allocation table");

      NandEntry root;
      require(nand_.Stat("/", &root) && root.IsDir() && root.name == "/" && root.index == 0 && root.size == 0,
              "Invalid NAND root entry");
      root_metadata_ = entry_metadata(root);

      struct Pending {
        NandEntry entry;
        QString path;
        int depth;
      };
      QList<Pending> pending{{root, QString(), 0}};
      QSet<quint16> ids{root.index};
      int visited = 0;
      while (!pending.isEmpty()) {
        const auto current = pending.takeLast();
        for (const auto& child : nand_.Children(current.entry.index)) {
          require(++visited <= maximum_entries && current.depth < maximum_depth,
                  "NAND directory tree exceeds inspection limits");
          require(!child.name.isEmpty() && child.name != "." && child.name != ".." && !child.name.contains('/'),
                  "Invalid NAND directory name");
          require((child.IsDir() || child.IsFile()) && child.size <= maximum_file_size &&
                      (child.IsFile() || child.size == 0) && !ids.contains(child.index),
                  "Invalid NAND entry metadata");
          ids.insert(child.index);
          const auto path = current.path + "/" + child.name;
          require(!entries_.contains(path), "Duplicate NAND path");
          entries_.insert(path, Entry{child.index, child.size, child.IsDir(), entry_metadata(child)});
          children_[current.path].insert(path);
          if (child.IsDir()) pending.push_back({child, path, current.depth + 1});
        }
      }
    }

    auto contains(const QString& path) const -> bool { return entries_.contains(path); }
    auto entry(const QString& path) const -> const Entry* {
      const auto it = entries_.constFind(path);
      return it == entries_.cend() ? nullptr : &it.value();
    }
    auto children(const QString& path) const -> QSet<QString> { return children_.value(path); }
    auto root_metadata() const -> const Metadata& { return root_metadata_; }

    auto read(const QString& path, qint64 limit) -> QByteArray {
      require(limit >= 0 && quint64(limit) <= maximum_title_size, "Invalid NAND read limit");
      require(!path.isEmpty() && path.startsWith('/') && QDir::cleanPath(path) == path,
              "Invalid NAND path");
      const auto* item = entry(path);
      require(item && !item->directory && item->size <= quint64(limit),
              "NAND file is missing or exceeds the inspection limit");
      const auto data = nand_.GetFile(item->id);
      require(quint64(data.size()) == item->size, "NAND file read was incomplete");
      return data;
    }

    auto files() const -> const QMap<QString, Entry>& { return entries_; }

  private:
    NandBin& nand_;
    Metadata root_metadata_;
    QMap<QString, Entry> entries_;
    QMap<QString, QSet<QString>> children_;
  };

  const QString menu_path = "/title/00000001/00000002";
  const QString menu_tmd_path = menu_path + "/content/title.tmd";
  const QString original_menu_tmd_path = menu_path + "/content/title_or.tmd";

  struct MenuBootContent {
    QString cid;
    quint16 index = 0;
    quint16 version = 0;
    quint64 size = 0;
    QByteArray sha1;
    bool nintendo_signed = false;
  };

  auto parse_menu_tmd(const QByteArray& bytes) -> std::optional<MenuBootContent> {
    if (bytes.size() < 0x1e4 || bytes.size() > 2 * 1024 * 1024 ||
        qFromBigEndian<quint32>(reinterpret_cast<const uchar*>(bytes.constData())) != ES_SIG_RSA2048 ||
        !bytes.mid(0x140, 0x40).contains('\0')) return std::nullopt;
    const auto count = qFromBigEndian<quint16>(reinterpret_cast<const uchar*>(bytes.constData() + 0x1de));
    if (bytes.size() != 0x1e4 + static_cast<int>(count) * 0x24) return std::nullopt;
    Tmd tmd(bytes);
    if (tmd.Tid() != 0x100000002ull || tmd.Count() != count ||
        tmd.SignedSize() != static_cast<quint32>(bytes.size())) return std::nullopt;
    QSet<QString> cids;
    QSet<quint16> indices;
    const auto boot_index = tmd.BootIndex();
    for (quint16 i = 0; i < count; ++i) {
      const auto cid = tmd.Cid(i);
      const auto index = tmd.Index(i);
      if (cid.size() != 8 || cids.contains(cid) || indices.contains(index)) return std::nullopt;
      cids.insert(cid);
      indices.insert(index);
      if (index == boot_index && tmd.Type(i) == 1)
        return MenuBootContent{cid, index, tmd.Version(), tmd.Size(i), tmd.Hash(i),
                               check_cert_chain(bytes) == ERROR_SUCCESS};
    }
    return std::nullopt;
  }

  auto read_optional(ImageIndex& index, const QString& path, qint64 limit) -> QByteArray {
    const auto* entry = index.entry(path);
    if (!entry || entry->directory || entry->size > quint64(limit)) return {};
    return index.read(path, limit);
  }

  auto same_content(const QByteArray& app, const MenuBootContent& expected) -> bool {
    return !app.isEmpty() && quint64(app.size()) == expected.size &&
           QCryptographicHash::hash(app, QCryptographicHash::Sha1) == expected.sha1;
  }

  auto collect_title_ids(const ImageIndex& index) -> QSet<quint64> {
    QSet<quint64> result;
    require(index.contains("/title") && index.entry("/title")->directory,
            "Title directory is missing");
    for (const auto& high_path : index.children("/title")) {
      const auto high_name = high_path.mid(QString("/title/").size());
      bool high_ok = false;
      const auto high = hex_id(high_name, high_ok);
      require(high_ok && index.entry(high_path)->directory, "Invalid title directory");
      for (const auto& low_path : index.children(high_path)) {
        const auto low_name = low_path.mid(high_path.size() + 1);
        bool low_ok = false;
        const auto low = hex_id(low_name, low_ok);
        require(low_ok && index.entry(low_path)->directory, "Invalid title directory");
        result.insert((quint64(high) << 32) | low);
      }
    }
    return result;
  }

  auto collect_ticket_ids(const ImageIndex& index) -> QSet<quint64> {
    QSet<quint64> result;
    require(index.contains("/ticket") && index.entry("/ticket")->directory,
            "Ticket directory is missing");
    for (const auto& high_path : index.children("/ticket")) {
      const auto high_name = high_path.mid(QString("/ticket/").size());
      bool high_ok = false;
      const auto high = hex_id(high_name, high_ok);
      require(high_ok && index.entry(high_path)->directory, "Invalid ticket directory");
      for (const auto& ticket_path : index.children(high_path)) {
        const auto file_name = ticket_path.mid(high_path.size() + 1);
        require(file_name.size() == 12 && file_name.endsWith(".tik"), "Invalid ticket file");
        const auto low_name = file_name.left(8);
        bool low_ok = false;
        const auto low = hex_id(low_name, low_ok);
        require(low_ok && index.entry(ticket_path) && !index.entry(ticket_path)->directory,
                "Invalid ticket file");
        result.insert((quint64(high) << 32) | low);
      }
    }
    return result;
  }

  void add_expected(QMap<QString, ExpectedMetadata>& expected, const QString& path, quint32 uid, quint16 gid,
                    int type, int user, int group, int other) {
    const ExpectedMetadata value{uid, gid, make_attr(type, user, group, other), 0, 0};
    const auto it = expected.constFind(path);
    if (it != expected.cend()) {
      require(it->uid == value.uid && it->gid == value.gid && it->attr == value.attr &&
                  it->x3 == value.x3 && it->mode == value.mode,
              "Conflicting generated NAND metadata");
      return;
    }
    expected.insert(path, value);
  }

  struct VerifiedTitle {
    QSet<QString> local_names{"title.tmd"};
    QMap<QString, QByteArray> shared_apps;
    quint64 ios = 0;
    quint16 gid = 0;
  };

  auto read_shared_map(ImageIndex& index) -> SharedContentMap {
    SharedContentMap shared(index.read("/shared1/content.map", 1024 * 1024));
    require(shared.Data().isEmpty() || shared.Check(), "Invalid shared content map");
    return shared;
  }

  auto verify_stock_title(ImageIndex& index, SharedContentMap& shared, quint64 id, quint16 version) -> VerifiedTitle {
    const auto id_text = title_path(id);
    const auto content_dir = "/title/" + id_text.left(8) + "/" + id_text.mid(8) + "/content";
    const auto metadata = index.read(content_dir + "/title.tmd", 2 * 1024 * 1024);
    const auto ticket = index.read("/ticket/" + id_text.left(8) + "/" + id_text.mid(8) + ".tik", 64 * 1024);
    require(CheckTitleMetadata(metadata, ticket) &&
                check_cert_chain(metadata) == ERROR_SUCCESS && check_cert_chain(ticket) == ERROR_SUCCESS,
            "Stock title metadata signature failed");
    Tmd tmd(metadata);
    Ticket tik(ticket, false);
    require(tmd.Tid() == id && tik.Tid() == id && tmd.Version() == version, "Stock title ID or version mismatch");

    VerifiedTitle result;
    result.ios = tmd.IOS();
    result.gid = tmd.Gid();
    NusJob job;
    job.tid = id;
    job.version = version;
    job.decrypt = true;
    job.data << metadata << ticket;
    quint64 total_content_size = 0;
    for (int i = 0; i < tmd.Count(); ++i) {
      const auto size = tmd.Size(i);
      require(size <= maximum_title_size && total_content_size + size <= maximum_title_size,
              "Stock title exceeds inspection limits");
      total_content_size += size;
      QString content_path;
      if (tmd.Type(i) == 0x8001) {
        const auto hash = tmd.Hash(i);
        const auto cid = shared.GetAppFromHash(hash);
        require(is_hex_id(cid), "Shared title content is absent from its map");
        const auto name = cid + ".app";
        result.shared_apps.insert(name, hash);
        content_path = "/shared1/" + name;
      } else {
        require(tmd.Type(i) == 1, "Unsupported stock content type");
        const auto cid = tmd.Cid(i);
        require(is_hex_id(cid), "Invalid stock content ID");
        const auto name = cid + ".app";
        result.local_names.insert(name);
        content_path = content_dir + "/" + name;
      }
      auto content = index.read(content_path, qint64(size));
      require(quint64(content.size()) == size, "Stock content size mismatch");
      job.data << content;
    }
    QSet<QString> actual_content_files;
    for (const auto& path : index.children(content_dir))
      actual_content_files.insert(path.mid(content_dir.size() + 1));
    require(actual_content_files == result.local_names, "Unexpected files in stock title content directory");
    require(decrypt_title(job), "Stock title content hash failed");
    return result;
  }
} // namespace

namespace inspect {
  auto priiloader_evidence(NandBin& nand) -> PriiloaderEvidence {
    ImageIndex index(nand);
    PriiloaderEvidence evidence;
    const auto active_tmd_bytes = read_optional(index, menu_tmd_path, 2 * 1024 * 1024);
    const auto active_tmd = parse_menu_tmd(active_tmd_bytes);
    if (!active_tmd) return evidence;

    evidence.menu_version = active_tmd->version;
    Tmd active_tmd_view(active_tmd_bytes);
    evidence.system_menu_ios = active_tmd_view.IOS();
    evidence.boot_app_path = menu_path + "/content/" + active_tmd->cid + ".app";
    const auto boot_app = read_optional(index, evidence.boot_app_path, 32 * 1024 * 1024);
    if (boot_app.isEmpty()) return evidence;
    const bool stock_boot = active_tmd->nintendo_signed && same_content(boot_app, *active_tmd);
    evidence.layout = stock_boot ? PriiloaderLayout::StockBoot : PriiloaderLayout::UnidentifiedBoot;

    if (!active_tmd->cid.startsWith('0')) return evidence;
    evidence.backup_app_path = menu_path + "/content/1" + active_tmd->cid.mid(1) + ".app";
    const auto backup_app = read_optional(index, evidence.backup_app_path, 32 * 1024 * 1024);
    const auto backup_tmd_bytes = read_optional(index, original_menu_tmd_path, 2 * 1024 * 1024);
    const auto original_tmd = parse_menu_tmd(backup_tmd_bytes);
    if (original_tmd) {
      evidence.backup_tmd_nintendo_signed = original_tmd->nintendo_signed;
      if (original_tmd->version == active_tmd->version && original_tmd->cid == active_tmd->cid &&
          original_tmd->index == active_tmd->index) {
        evidence.backup_app_matches_original_tmd_hash =
            !backup_app.isEmpty() && QCryptographicHash::hash(backup_app, QCryptographicHash::Sha1) == original_tmd->sha1;
        evidence.backup_app_matches_original_tmd_size =
            !backup_app.isEmpty() && quint64(backup_app.size()) == original_tmd->size;
      }
    }
    const bool matches_active_tmd = same_content(backup_app, *active_tmd) && active_tmd->nintendo_signed;
    const bool matches_original_tmd = evidence.backup_tmd_nintendo_signed &&
        evidence.backup_app_matches_original_tmd_hash && evidence.backup_app_matches_original_tmd_size;
    if (!stock_boot && (matches_active_tmd || matches_original_tmd) && boot_app != backup_app)
      evidence.layout = PriiloaderLayout::Installed;
    return evidence;
  }

  void verify_stock_titles(NandBin& nand, const QMap<quint64, quint16>& titles) {
    ImageIndex index(nand);
    auto shared = read_shared_map(index);
    for (auto title = titles.cbegin(); title != titles.cend(); ++title) {
      const auto verified = verify_stock_title(index, shared, title.key(), title.value());
      if (verified.ios) require(titles.contains(verified.ios), "Stock title dependency is missing");
    }
  }

  void verify_filesystem(NandBin& nand, bool generated, quint16 active_metadata) {
    const auto fats = nand.GetFats();
    require(fats.size() == 0x8000, "Invalid NAND allocation table");
    for (int cluster = 0x40; cluster < 0x7f00; ++cluster) {
      if (fats[cluster] == 0xfffd || fats[cluster] == 0xfffe) continue;
      for (int page = 0; page < 8; ++page)
        require(nand.CheckEcc(cluster * 8 + page), "Filesystem ECC check failed");
    }
    require(active_metadata >= 0x7f00 && active_metadata <= 0x7ff0 && active_metadata % 16 == 0,
            "Invalid active metadata address");
    if (generated) {
      for (int cluster = 0x7f00; cluster < 0x8000; cluster += 16) {
        for (int page = 0; page < 16 * 8; ++page)
          require(nand.CheckEcc(cluster * 8 + page), "Generated metadata ECC check failed");
        require(nand.CheckHmacMeta(cluster), "Generated metadata authentication failed");
      }
    } else {
      for (int page = 0; page < 16 * 8; ++page)
        require(nand.CheckEcc(active_metadata * 8 + page), "Active metadata ECC check failed");
      require(nand.CheckHmacMeta(active_metadata), "Active metadata authentication failed");
    }
    ImageIndex index(nand);
    for (auto it = index.files().cbegin(); it != index.files().cend(); ++it) {
      const auto& entry = it.value();
      if (entry.directory) continue;
      require(nand.CheckHmacData(entry.id), "Filesystem file authentication failed");
    }
  }

  void verify_stock(NandBin& nand, const QString& area) {
    ImageIndex index(nand);
    const auto profile = factory::stock_titles(area);
    QSet<quint64> expected_titles;
    UIDmap expected_uid;
    QMap<quint64, quint32> title_uids;
    for (auto it = profile.cbegin(); it != profile.cend(); ++it) {
      expected_titles.insert(it.key());
      const auto uid = expected_uid.GetUid(it.key());
      require(uid != 0, "Cannot construct expected stock UID map");
      title_uids.insert(it.key(), uid);
    }
    require(!expected_titles.isEmpty() && expected_uid.Check(),
            "Cannot construct expected stock UID map");
    require(collect_title_ids(index) == expected_titles &&
                collect_ticket_ids(index) == expected_titles,
            "Installed title or ticket set does not match the pinned stock profile");
    require(index.read("/sys/uid.sys", 1024 * 1024) == expected_uid.Data(),
            "UID map does not exactly match stock title assignments");
    require(index.read("/sys/cert.sys", CERTS_DAT_SIZE) ==
                QByteArray(reinterpret_cast<const char*>(certs_dat), CERTS_DAT_SIZE),
            "System certificate file differs from the generated stock layout");

    QMap<QString, ExpectedMetadata> expected_metadata;
    add_expected(expected_metadata, "/sys", 0, 0, NAND_DIR, NAND_RW, NAND_RW, 0);
    add_expected(expected_metadata, "/ticket", 0, 0, NAND_DIR, NAND_RW, NAND_RW, 0);
    add_expected(expected_metadata, "/title", 0, 0, NAND_DIR, NAND_RW, NAND_RW, NAND_READ);
    add_expected(expected_metadata, "/shared1", 0, 0, NAND_DIR, NAND_RW, NAND_RW, 0);
    add_expected(expected_metadata, "/shared2", 0, 0, NAND_DIR, NAND_RW, NAND_RW, NAND_RW);
    add_expected(expected_metadata, "/shared2/sys", 0, 0, NAND_DIR, NAND_RW, NAND_RW, NAND_RW);
    add_expected(expected_metadata, "/import", 0, 0, NAND_DIR, NAND_RW, NAND_RW, 0);
    add_expected(expected_metadata, "/meta", 0x1000, 1, NAND_DIR, NAND_RW, NAND_RW, NAND_RW);
    add_expected(expected_metadata, "/tmp", 0, 0, NAND_DIR, NAND_RW, NAND_RW, NAND_RW);
    add_expected(expected_metadata, "/sys/cert.sys", 0, 0, NAND_FILE, NAND_RW, NAND_RW, NAND_READ);
    add_expected(expected_metadata, "/sys/uid.sys", 0, 0, NAND_FILE, NAND_RW, NAND_RW, 0);
    add_expected(expected_metadata, "/shared1/content.map", 0, 0, NAND_FILE, NAND_RW, NAND_RW, 0);
    add_expected(expected_metadata, "/title/00000001/00000002/data/setting.txt", 0x1000, 1,
                 NAND_FILE, NAND_READ, NAND_READ, NAND_READ);

    auto shared = read_shared_map(index);
    QSet<QByteArray> expected_shared_hashes;
    QSet<QString> expected_shared_files{"content.map"};
    for (auto title = profile.cbegin(); title != profile.cend(); ++title) {
      const auto id = title.key();
      const auto id_text = title_path(id);
      const auto high = id_text.left(8);
      const auto low = id_text.mid(8);
      const auto base = "/title/" + high + "/" + low;
      const auto ticket_dir = "/ticket/" + high;
      const auto content_dir = base + "/content";

      add_expected(expected_metadata, ticket_dir, 0, 0, NAND_DIR, NAND_RW, NAND_RW, 0);
      add_expected(expected_metadata, "/title/" + high, 0, 0, NAND_DIR, NAND_RW, NAND_RW, NAND_READ);
      add_expected(expected_metadata, base, 0, 0, NAND_DIR, NAND_RW, NAND_RW, NAND_READ);
      add_expected(expected_metadata, content_dir, 0, 0, NAND_DIR, NAND_RW, NAND_RW, 0);
      add_expected(expected_metadata, ticket_dir + "/" + low + ".tik", 0, 0,
                   NAND_FILE, NAND_RW, NAND_RW, 0);

      const auto verified = verify_stock_title(index, shared, id, title.value());
      add_expected(expected_metadata, base + "/data", title_uids.value(id), verified.gid,
                   NAND_DIR, NAND_RW, 0, 0);
      if (verified.ios)
        require(expected_titles.contains(verified.ios), "Stock title dependency is missing");
      for (const auto& name : verified.local_names)
        add_expected(expected_metadata, content_dir + "/" + name, 0, 0,
                     NAND_FILE, NAND_RW, NAND_RW, 0);
      for (auto app = verified.shared_apps.cbegin(); app != verified.shared_apps.cend(); ++app) {
        expected_shared_files.insert(app.key());
        expected_shared_hashes.insert(app.value());
        add_expected(expected_metadata, "/shared1/" + app.key(), 0, 0,
                     NAND_FILE, NAND_RW, NAND_RW, 0);
      }
    }

    QSet<QString> actual_shared_files;
    for (const auto& path : index.children("/shared1")) {
      const auto* entry = index.entry(path);
      require(entry && !entry->directory, "Unexpected shared content directory");
      actual_shared_files.insert(path.mid(QString("/shared1/").size()));
    }
    require(actual_shared_files == expected_shared_files,
            "Shared content files do not match installed stock titles");
    require(shared.Count() == expected_shared_hashes.size(),
            "Shared content map contains unused entries");

    const auto root = index.root_metadata();
    require(root.uid == 0 && root.gid == 0 && root.x3 == 0 && root.mode == 0 && root.attr == 0x16,
            "NAND root metadata does not match the generated stock layout");

    QSet<QString> actual_paths;
    for (auto it = index.files().cbegin(); it != index.files().cend(); ++it)
      actual_paths.insert(it.key());
    QSet<QString> expected_paths;
    for (auto it = expected_metadata.cbegin(); it != expected_metadata.cend(); ++it)
      expected_paths.insert(it.key());
    require(actual_paths == expected_paths,
            "NAND paths do not match the generated stock layout");
    for (auto it = expected_metadata.cbegin(); it != expected_metadata.cend(); ++it) {
      const auto* entry = index.entry(it.key());
      const auto& want = it.value();
      require(entry && entry->directory == ((want.attr & 3) == NAND_DIR),
              "NAND entry type differs from the generated stock layout");
      require(entry && entry->metadata.uid == want.uid && entry->metadata.gid == want.gid &&
                  entry->metadata.x3 == want.x3 && entry->metadata.mode == want.mode &&
                  entry->metadata.attr == want.attr,
              "NAND entry metadata differs from the generated stock layout");
    }
  }
} // namespace inspect
