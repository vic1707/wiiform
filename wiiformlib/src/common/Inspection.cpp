// SPDX-License-Identifier: GPL-2.0-only
#include <Inspection.hpp>

#include <Factory.hpp>
#include <PriiloaderHacks.hpp>
#include <PriiloaderSettings.hpp>
#include <Titles.hpp>
#include <sharedcontentmap.h>
#include <tiktmd.h>
#include <uidmap.h>

#include <QCryptographicHash>
#include <QDir>
#include <QJsonArray>
#include <QJsonObject>
#include <QMap>
#include <QSet>
#include <QTreeWidgetItem>
#include <QtEndian>
#include <algorithm>
#include <memory>
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

  auto hex_id(const QString& text, bool& ok) -> quint32 {
    if (!is_hex_id(text)) {
      ok = false;
      return 0;
    }
    return text.toUInt(&ok, 16);
  }

  auto entry_type(const QTreeWidgetItem* item, bool& ok) -> int {
    return item->text(7).left(2).toInt(&ok, 16);
  }

  auto entry_id(const QTreeWidgetItem* item, bool& ok) -> quint16 {
    const auto value = item->text(1).toUInt(&ok, 10);
    if (!ok || value > 0xffff) ok = false;
    return static_cast<quint16>(value);
  }

  auto entry_size(const QTreeWidgetItem* item, bool& ok) -> quint64 {
    const auto value = item->text(2).toULongLong(&ok, 16);
    if (!ok || value > maximum_file_size) ok = false;
    return value;
  }

  struct Metadata {
    quint32 uid = 0;
    quint16 gid = 0;
    quint32 x3 = 0;
    quint8 mode = 0;
    quint8 attr = 0;
  };

  auto entry_metadata(const QTreeWidgetItem* item, bool& ok) -> Metadata {
    Metadata metadata;
    bool uid_ok = false;
    bool gid_ok = false;
    bool x3_ok = false;
    bool mode_ok = false;
    bool attr_ok = false;
    metadata.uid = item->text(3).toUInt(&uid_ok, 16);
    metadata.gid = item->text(4).left(4).toUShort(&gid_ok, 16);
    metadata.x3 = item->text(5).toUInt(&x3_ok, 16);
    const auto mode = item->text(6).toUInt(&mode_ok, 16);
    const auto attr_text = item->text(7);
    const auto open = attr_text.lastIndexOf('[');
    require(open >= 0 && attr_text.endsWith(']'), "Invalid NAND attribute field");
    const auto attr = attr_text.mid(open + 1, attr_text.size() - open - 2).toUInt(&attr_ok, 16);
    require(mode <= 0xff && attr <= 0xff, "Invalid NAND metadata value");
    metadata.mode = static_cast<quint8>(mode);
    metadata.attr = static_cast<quint8>(attr);
    ok = uid_ok && gid_ok && x3_ok && mode_ok && attr_ok;
    return metadata;
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

      std::unique_ptr<QTreeWidgetItem> tree(nand_.GetTree());
      require(tree && tree->childCount() == 1, "Invalid NAND root");
      auto* root = tree->child(0);
      bool type_ok = false;
      require(entry_type(root, type_ok) == NAND_DIR && type_ok, "Invalid NAND root");
      bool root_id_ok = false;
      bool root_size_ok = false;
      bool root_metadata_ok = false;
      const auto root_id = entry_id(root, root_id_ok);
      const auto root_size = entry_size(root, root_size_ok);
      root_metadata_ = entry_metadata(root, root_metadata_ok);
      require(root->text(0) == "/" && root_id_ok && root_id == 0 && root_size_ok && root_size == 0 &&
                  root_metadata_ok,
              "Invalid NAND root entry");

      struct Pending {
        QTreeWidgetItem* item;
        QString path;
        int depth;
      };
      QList<Pending> pending;
      for (int i = 0; i < root->childCount(); ++i) {
        auto* item = root->child(i);
        const auto name = item->text(0);
        require(!name.isEmpty() && name != "." && name != ".." && !name.contains('/'),
                "Invalid NAND root entry name");
        pending.push_back({item, "/" + name, 1});
      }
      QSet<quint16> ids;
      int visited = 0;
      while (!pending.isEmpty()) {
        const auto current = pending.takeLast();
        require(++visited <= maximum_entries && current.depth <= maximum_depth,
                "NAND directory tree exceeds inspection limits");
        require(current.item && !current.path.endsWith('/') && !current.path.contains("//") &&
                    current.path != "/." && current.path != "/..",
                "Invalid NAND directory entry");

        bool id_ok = false;
        bool size_ok = true;
        bool kind_ok = false;
        const auto id = entry_id(current.item, id_ok);
        const auto kind = entry_type(current.item, kind_ok);
        bool metadata_ok = false;
        const auto metadata = entry_metadata(current.item, metadata_ok);
        const bool is_directory = kind == NAND_DIR;
        const auto size = is_directory ? quint64(0) : entry_size(current.item, size_ok);
        require(id_ok && size_ok && kind_ok && metadata_ok && (metadata.attr & 3) == kind &&
                    !ids.contains(id),
                "Invalid NAND entry metadata");
        ids.insert(id);
        require(is_directory || kind == NAND_FILE, "Unsupported NAND entry type");
        if (is_directory) require(size == 0, "Invalid NAND directory size");
        else require(current.item->childCount() == 0, "Invalid NAND file entry");
        require(!entries_.contains(current.path), "Duplicate NAND path");
        entries_.insert(current.path, Entry{id, size, is_directory, metadata});

        const auto parent = current.path.left(current.path.lastIndexOf('/'));
        children_[parent].insert(current.path);
        if (is_directory) {
          for (int i = 0; i < current.item->childCount(); ++i) {
            auto* item = current.item->child(i);
            const auto name = item->text(0);
            require(!name.isEmpty() && name != "." && name != ".." && !name.contains('/'),
                    "Invalid NAND directory name");
            pending.push_back({item, current.path + "/" + name, current.depth + 1});
          }
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
      const auto data = nand_.GetData(path);
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

  auto title_path(quint64 id) -> QString {
    return QString("%1").arg(id, 16, 16, QChar('0'));
  }

  auto hex_title_id(quint64 value) -> QString {
    return QString("%1").arg(value, 16, 16, QChar('0'));
  }

  auto hash_bytes(const QByteArray& bytes, QCryptographicHash::Algorithm algorithm) -> QString {
    return QString::fromLatin1(QCryptographicHash::hash(bytes, algorithm).toHex());
  }

  auto inventory_uid_map(const QByteArray& bytes) -> QJsonObject {
    require(bytes.size() > 0 && bytes.size() % 12 == 0, "Malformed UID map length");
    QSet<quint64> title_ids;
    QSet<quint32> uids;
    QList<QPair<quint64, quint32>> parsed;
    for (int offset = 0; offset < bytes.size(); offset += 12) {
      const auto* raw = reinterpret_cast<const uchar*>(bytes.constData() + offset);
      const auto title = qFromBigEndian<quint64>(raw);
      const auto uid = qFromBigEndian<quint32>(raw + 8);
      require(title != 0 && uid != 0 && !title_ids.contains(title) && !uids.contains(uid),
              "Malformed UID map record");
      title_ids.insert(title);
      uids.insert(uid);
      parsed.append({title, uid});
    }
    std::sort(parsed.begin(), parsed.end(), [](const auto& left, const auto& right) {
      return left.first < right.first;
    });
    QJsonArray records;
    for (const auto& item : parsed)
      records.append(QJsonObject{{"title_id", hex_title_id(item.first)},
                                 {"uid", static_cast<qint64>(item.second)}});
    return QJsonObject{{"present", true}, {"records", records}};
  }

  auto inventory_shared_map(const QByteArray& bytes) -> QJsonObject {
    QJsonArray records;
    require(bytes.size() % 28 == 0, "Malformed shared-content map length");
    QSet<QString> names;
    for (int offset = 0; offset < bytes.size(); offset += 28) {
      const auto name_bytes = bytes.mid(offset, 8);
      bool ok = false;
      const auto number = name_bytes.toUInt(&ok, 16);
      const auto name = QString::fromLatin1(name_bytes);
      require(is_hex_id(name) && ok && number == static_cast<quint32>(offset / 28) && !names.contains(name),
              "Malformed shared-content map record");
      names.insert(name);
      records.append(QJsonObject{{"name", name},
                                 {"sha1", QString::fromLatin1(bytes.mid(offset + 8, 20).toHex())}});
    }
    return QJsonObject{{"present", true}, {"records", records}};
  }

  auto inventory_title(const QString& path, const QByteArray& bytes) -> QJsonObject {
    // Check the count and exact table size before constructing WiiQt's legacy
    // Tmd view, whose constructor trusts those fields when it sets pointers.
    require(bytes.size() >= 0x1e4 &&
                qFromBigEndian<quint32>(reinterpret_cast<const uchar*>(bytes.constData())) == ES_SIG_RSA2048 &&
                bytes.mid(0x140, 0x40).contains('\0'),
            "Malformed title metadata header");
    const auto count = qFromBigEndian<quint16>(reinterpret_cast<const uchar*>(bytes.constData() + 0x1de));
    require(bytes.size() == 0x1e4 + static_cast<int>(count) * 0x24,
            "Malformed title metadata content table");
    Tmd tmd(bytes);
    const auto title = tmd.Tid();
    require(title != 0 && tmd.Count() == count && tmd.SignedSize() == static_cast<quint32>(bytes.size()),
            "Malformed title metadata fields");
    const auto parts = path.split('/');
    require(parts.size() == 6 && parts[1] == "title" && is_hex_id(parts[2]) && is_hex_id(parts[3]) &&
                parts[4] == "content" && parts[5] == "title.tmd",
            "Malformed title metadata path");
    bool high_ok = false;
    bool low_ok = false;
    const auto path_title = (quint64(parts[2].toUInt(&high_ok, 16)) << 32) | parts[3].toUInt(&low_ok, 16);
    require(high_ok && low_ok && path_title == title, "Title metadata ID does not match its path");
    QSet<QString> content_ids;
    QSet<quint16> content_indices;
    for (quint16 i = 0; i < count; ++i) {
      const auto cid = tmd.Cid(i);
      const auto index = tmd.Index(i);
      // The TMD's declared size is a metadata fact, not an input to this
      // inventory's bounded reads or allocations. Modified titles can declare
      // content sizes outside a stock NAND profile.
      require(cid.size() == 8 && !content_ids.contains(cid) && !content_indices.contains(index),
              "Malformed title metadata content entries");
      content_ids.insert(cid);
      content_indices.insert(index);
    }
    return QJsonObject{{"path", path}, {"title_id", hex_title_id(title)}, {"version", tmd.Version()},
                       {"ios", hex_title_id(tmd.IOS())}, {"content_count", count},
                       {"tmd_sha256", hash_bytes(bytes, QCryptographicHash::Sha256)}};
  }

  const QString menu_path = "/title/00000001/00000002";
  const QString menu_tmd_path = menu_path + "/content/title.tmd";

  struct FileProbe {
    QJsonObject evidence;
    QByteArray bytes;
  };

  auto probe_file(ImageIndex& index, const QString& path, qint64 limit) -> FileProbe {
    QJsonObject evidence{{"path", path}};
    const auto* entry = index.entry(path);
    if (!entry) {
      evidence["status"] = "missing";
      return {evidence, {}};
    }
    if (entry->directory) {
      evidence["status"] = "not-a-file";
      return {evidence, {}};
    }
    evidence["size"] = static_cast<qint64>(entry->size);
    if (entry->size > quint64(limit)) {
      evidence["status"] = "size-limit";
      return {evidence, {}};
    }
    auto bytes = index.read(path, limit);
    evidence["status"] = "present";
    evidence["sha256"] = hash_bytes(bytes, QCryptographicHash::Sha256);
    return {evidence, bytes};
  }

  struct MenuBootContent {
    QString cid;
    quint16 index;
    quint16 version;
    quint64 size;
    QByteArray sha1;
    bool nintendo_signed;
  };

  auto probe_menu_tmd(FileProbe& file) -> std::optional<MenuBootContent> {
    if (file.evidence["status"].toString() != "present") return std::nullopt;
    try {
      // A backup TMD must identify the same title as the active TMD.
      const auto metadata = inventory_title(menu_tmd_path, file.bytes);
      for (const auto* key : {"version", "ios", "content_count"})
        file.evidence[key] = metadata[key];
    } catch (const std::runtime_error&) {
      file.evidence["status"] = "malformed";
      return std::nullopt;
    }
    const bool signed_tmd = check_cert_chain(file.bytes) == ERROR_SUCCESS;
    file.evidence["nintendo_signed"] = signed_tmd;
    Tmd tmd(file.bytes);
    const auto boot_index = tmd.BootIndex();
    for (quint16 i = 0; i < tmd.Count(); ++i) {
      if (tmd.Index(i) != boot_index || tmd.Type(i) != 1) continue;
      file.evidence["boot_index"] = boot_index;
      file.evidence["boot_cid"] = tmd.Cid(i);
      return MenuBootContent{tmd.Cid(i), boot_index, tmd.Version(), tmd.Size(i), tmd.Hash(i), signed_tmd};
    }
    file.evidence["status"] = "unsupported-boot-content";
    return std::nullopt;
  }

  struct BootApp {
    QJsonObject evidence;
    QByteArray sha1;
    quint64 size;
  };

  auto probe_boot_app(ImageIndex& index, const QString& cid) -> BootApp {
    const auto file = probe_file(index, menu_path + "/content/" + cid + ".app", 32 * 1024 * 1024);
    const auto sha1 = file.evidence["status"].toString() == "present"
                          ? QCryptographicHash::hash(file.bytes, QCryptographicHash::Sha1) : QByteArray();
    return {file.evidence, sha1, static_cast<quint64>(file.bytes.size())};
  }

  auto compare_boot_app(BootApp& app, const MenuBootContent& expected,
                        const QString& label = "tmd") -> bool {
    if (app.sha1.isEmpty()) return false;
    const bool hash_matches = app.sha1 == expected.sha1;
    const bool size_matches = app.size == expected.size;
    app.evidence["matches_" + label + "_hash"] = hash_matches;
    app.evidence["matches_" + label + "_size"] = size_matches;
    return hash_matches && size_matches;
  }

  auto probe_menu_boot(ImageIndex& index) -> QJsonObject {
    auto tmd = probe_file(index, menu_tmd_path, 2 * 1024 * 1024);
    auto backup_tmd = probe_file(index, menu_path + "/content/title_or.tmd", 2 * 1024 * 1024);
    const auto boot = probe_menu_tmd(tmd);
    const auto original = probe_menu_tmd(backup_tmd);
    QJsonObject result{{"tmd", tmd.evidence}, {"backup_tmd", backup_tmd.evidence},
                       {"status", "system-menu-unavailable"}};
    if (!boot) return result;

    auto app = probe_boot_app(index, boot->cid);
    const bool stock_app = compare_boot_app(app, *boot) && boot->nintendo_signed;
    result["boot_app"] = app.evidence;
    if (app.size != 0) result["status"] = stock_app ? "stock-boot" : "unidentified-boot";
    // The Wii installer substitutes 1 for the first digit of the stock CID.
    // A signed TMD must identify the relocated app as the original menu.
    if (!boot->cid.startsWith('0')) return result;
    auto backup = probe_boot_app(index, "1" + boot->cid.mid(1));
    bool stock_backup = compare_boot_app(backup, *boot) && boot->nintendo_signed;
    if (original && original->version == boot->version &&
        original->cid == boot->cid && original->index == boot->index) {
      const bool matches_original = compare_boot_app(backup, *original, "original_tmd");
      stock_backup = stock_backup || (matches_original && original->nintendo_signed);
    }
    result["backup_app"] = backup.evidence;
    if (!stock_app && stock_backup && app.size != 0 && app.sha1 != backup.sha1)
      result["status"] = "priiloader-layout";
    return result;
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

  auto verify_expected_metadata(ImageIndex& index, const QString& area) -> inspect::MetadataSummary {
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
    require(expected_uid.Check(), "Cannot construct expected stock UID map");
    require(collect_title_ids(index) == expected_titles && collect_ticket_ids(index) == expected_titles,
            "Installed title or ticket set does not match the pinned stock profile");
    require(index.read("/sys/uid.sys", 1024 * 1024) == expected_uid.Data(),
            "UID map does not exactly match stock title assignments");

    QMap<QString, ExpectedMetadata> expected;
    add_expected(expected, "/sys", 0, 0, NAND_DIR, NAND_RW, NAND_RW, 0);
    add_expected(expected, "/ticket", 0, 0, NAND_DIR, NAND_RW, NAND_RW, 0);
    add_expected(expected, "/title", 0, 0, NAND_DIR, NAND_RW, NAND_RW, NAND_READ);
    add_expected(expected, "/shared1", 0, 0, NAND_DIR, NAND_RW, NAND_RW, 0);
    add_expected(expected, "/shared2", 0, 0, NAND_DIR, NAND_RW, NAND_RW, NAND_RW);
    add_expected(expected, "/shared2/sys", 0, 0, NAND_DIR, NAND_RW, NAND_RW, NAND_RW);
    add_expected(expected, "/import", 0, 0, NAND_DIR, NAND_RW, NAND_RW, 0);
    add_expected(expected, "/meta", 0x1000, 1, NAND_DIR, NAND_RW, NAND_RW, NAND_RW);
    add_expected(expected, "/tmp", 0, 0, NAND_DIR, NAND_RW, NAND_RW, NAND_RW);
    add_expected(expected, "/sys/cert.sys", 0, 0, NAND_FILE, NAND_RW, NAND_RW, NAND_READ);
    add_expected(expected, "/sys/uid.sys", 0, 0, NAND_FILE, NAND_RW, NAND_RW, 0);
    add_expected(expected, "/shared1/content.map", 0, 0, NAND_FILE, NAND_RW, NAND_RW, 0);
    add_expected(expected, "/title/00000001/00000002/data/setting.txt", 0x1000, 1,
                 NAND_FILE, NAND_READ, NAND_READ, NAND_READ);

    QSet<QByteArray> expected_shared_hashes;
    QSet<QString> expected_shared_paths;
    SharedContentMap shared(index.read("/shared1/content.map", 1024 * 1024));
    require(shared.Data().isEmpty() || shared.Check(), "Invalid shared content map");

    for (auto it = profile.cbegin(); it != profile.cend(); ++it) {
      const auto id = it.key();
      const auto id_text = title_path(id);
      const auto high = id_text.left(8);
      const auto low = id_text.mid(8);
      const auto base = "/title/" + high + "/" + low;
      const auto ticket_dir = "/ticket/" + high;
      const auto content_dir = base + "/content";
      add_expected(expected, ticket_dir, 0, 0, NAND_DIR, NAND_RW, NAND_RW, 0);
      add_expected(expected, "/title/" + high, 0, 0, NAND_DIR, NAND_RW, NAND_RW, NAND_READ);
      add_expected(expected, base, 0, 0, NAND_DIR, NAND_RW, NAND_RW, NAND_READ);
      add_expected(expected, content_dir, 0, 0, NAND_DIR, NAND_RW, NAND_RW, 0);
      add_expected(expected, ticket_dir + "/" + low + ".tik", 0, 0, NAND_FILE, NAND_RW, NAND_RW, 0);
      add_expected(expected, content_dir + "/title.tmd", 0, 0, NAND_FILE, NAND_RW, NAND_RW, 0);

      const auto tmd_data = index.read(content_dir + "/title.tmd", 2 * 1024 * 1024);
      const auto ticket_data = index.read(ticket_dir + "/" + low + ".tik", 64 * 1024);
      require(check_title_metadata(tmd_data, ticket_data), "Malformed stock title metadata");
      Tmd tmd(tmd_data);
      require(tmd.Tid() == id && tmd.Version() == it.value(), "Stock title ID or version mismatch");
      add_expected(expected, base + "/data", title_uids.value(id), tmd.Gid(), NAND_DIR, NAND_RW, 0, 0);
      quint64 total_size = 0;
      for (int content_index = 0; content_index < tmd.Count(); ++content_index) {
        const auto size = tmd.Size(content_index);
        require(size <= maximum_title_size && total_size + size <= maximum_title_size,
                "Stock title exceeds inspection limits");
        total_size += size;
        QString content_path;
        if (tmd.Type(content_index) == 0x8001) {
          const auto hash = tmd.Hash(content_index);
          const auto cid = shared.GetAppFromHash(hash);
          require(is_hex_id(cid), "Shared title content is absent from its map");
          content_path = "/shared1/" + cid + ".app";
          expected_shared_paths.insert(content_path);
          expected_shared_hashes.insert(hash);
        } else {
          require(tmd.Type(content_index) == 1, "Unsupported stock content type");
          const auto cid = tmd.Cid(content_index);
          require(is_hex_id(cid), "Invalid stock content ID");
          content_path = content_dir + "/" + cid + ".app";
        }
        add_expected(expected, content_path, 0, 0, NAND_FILE, NAND_RW, NAND_RW, 0);
        const auto* file = index.entry(content_path);
        require(file && !file->directory && file->size == size, "Stock content size mismatch");
      }
    }
    require(shared.Count() == expected_shared_hashes.size(), "Shared content map contains unused entries");
    for (const auto& shared_path : expected_shared_paths)
      require(index.contains(shared_path), "Shared content file is missing");

    const auto root = index.root_metadata();
    require(root.uid == 0 && root.gid == 0 && root.x3 == 0 && root.mode == 0 && root.attr == 0x16,
            "NAND root metadata does not match the generated stock layout");

    QSet<QString> actual_paths;
    for (auto it = index.files().cbegin(); it != index.files().cend(); ++it) actual_paths.insert(it.key());
    QSet<QString> expected_paths;
    for (auto it = expected.cbegin(); it != expected.cend(); ++it) expected_paths.insert(it.key());
    require(actual_paths == expected_paths, "NAND paths do not match the generated stock layout");
    for (auto it = expected.cbegin(); it != expected.cend(); ++it) {
      const auto* entry = index.entry(it.key());
      const auto& want = it.value();
      require(entry && entry->metadata.uid == want.uid && entry->metadata.gid == want.gid &&
                  entry->metadata.x3 == want.x3 && entry->metadata.mode == want.mode &&
                  entry->metadata.attr == want.attr,
              "NAND entry metadata differs from the generated stock layout");
    }
    return {expected.size() + 1, profile.size()};
  }

  struct VerifiedTitle {
    QSet<QString> local_names{"title.tmd"};
    QMap<QString, QByteArray> shared_apps;
    quint64 ios = 0;
    int content_count = 0;
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
    require(check_title_metadata(metadata, ticket) &&
                check_cert_chain(metadata) == ERROR_SUCCESS && check_cert_chain(ticket) == ERROR_SUCCESS,
            "Stock title metadata signature failed");
    Tmd tmd(metadata);
    Ticket tik(ticket, false);
    require(tmd.Tid() == id && tik.Tid() == id && tmd.Version() == version, "Stock title ID or version mismatch");

    VerifiedTitle result;
    result.ios = tmd.IOS();
    result.content_count = tmd.Count();
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
  auto read_file(NandBin& nand, const QString& path, qint64 limit) -> QByteArray {
    ImageIndex index(nand);
    return index.read(path, limit);
  }

  auto priiloader_evidence(NandBin& nand) -> QJsonObject {
    ImageIndex index(nand);
    auto menu = probe_menu_boot(index);
    const auto status = menu.take("status");
    QJsonObject artifacts;
    QJsonObject settings{{"status", "missing"}};
    QByteArray hack_definitions, hack_states;
    const QStringList names{"loader.ini", "hacks.ini", "hacks_s.ini", "hackshas.ini", "hacksh_s.ini"};
    for (const auto& name : names) {
      const auto file = probe_file(index, menu_path + "/data/" + name, 1024 * 1024);
      const auto file_status = file.evidence["status"].toString();
      if (file_status == "missing") continue;
      artifacts[name] = file.evidence;
      if (name == "loader.ini") {
        if (file_status == "present") settings = priiloader::inspect_settings(file.bytes);
        else settings = QJsonObject{{"status", "unsupported"}, {"reason", file_status}};
      }
      if (name == "hackshas.ini") hack_definitions = file.bytes;
      if (name == "hacksh_s.ini") hack_states = file.bytes;
    }
    QJsonObject report{{"status", status}, {"system_menu", menu}, {"artifacts", artifacts}, {"settings", settings}};
    if (!hack_definitions.isEmpty() && !hack_states.isEmpty()) {
      const int version = menu["tmd"].toObject()["version"].toInt();
      try {
        report["hacks"] = priiloader::inspect_hacks(hack_definitions, hack_states, version);
      } catch (const std::exception&) {
        // Keep authenticated file evidence available for dumps built with other
        // Priiloader releases whose source definitions are not pinned here.
        report["hacks_status"] = "unrecognized-definitions";
      }
    }
    return report;
  }

  auto inventory_filesystem(NandBin& nand) -> QJsonObject {
    ImageIndex index(nand);
    QJsonArray entries;
    const auto& root = index.root_metadata();
    entries.append(QJsonObject{{"path", "/"}, {"type", "directory"}, {"size", 0},
                               {"uid", static_cast<qint64>(root.uid)}, {"gid", root.gid},
                               {"attr", root.attr}, {"mode", root.mode},
                               {"x3", static_cast<qint64>(root.x3)}, {"sha256", QJsonValue(QJsonValue::Null)},
                               {"fst_index", 0}, {"clusters", QJsonArray()}});

    QJsonArray titles;
    quint64 total_file_bytes = 0;
    for (auto it = index.files().cbegin(); it != index.files().cend(); ++it) {
      const auto& path = it.key();
      require(!path.contains(QChar::ReplacementCharacter), "Unsupported NAND filename encoding");
      const auto& entry = it.value();
      QJsonArray clusters;
      QJsonValue file_hash(QJsonValue::Null);
      if (!entry.directory) {
        total_file_bytes += entry.size;
        require(total_file_bytes <= maximum_title_size, "NAND live-file data exceeds inventory limits");
        const auto data = index.read(path, static_cast<qint64>(maximum_title_size));
        file_hash = hash_bytes(data, QCryptographicHash::Sha256);
        const auto file_clusters = nand.GetFatsForFile(entry.id);
        const auto expected_clusters = (entry.size + 0x3fff) / 0x4000;
        require(static_cast<quint64>(file_clusters.size()) == expected_clusters,
                "NAND file allocation differs from its declared size");
        for (const auto cluster : file_clusters) clusters.append(cluster);
        if (path.endsWith("/content/title.tmd")) titles.append(inventory_title(path, data));
      }
      entries.append(QJsonObject{{"path", path},
                                 {"type", entry.directory ? "directory" : "file"},
                                 {"size", static_cast<qint64>(entry.size)},
                                 {"uid", static_cast<qint64>(entry.metadata.uid)},
                                 {"gid", entry.metadata.gid}, {"attr", entry.metadata.attr},
                                 {"mode", entry.metadata.mode},
                                 {"x3", static_cast<qint64>(entry.metadata.x3)},
                                 {"sha256", file_hash}, {"fst_index", entry.id}, {"clusters", clusters}});
    }

    QJsonObject maps;
    maps["uid"] = index.contains("/sys/uid.sys")
                       ? inventory_uid_map(index.read("/sys/uid.sys", 1024 * 1024))
                       : QJsonObject{{"present", false}, {"records", QJsonArray()}};
    maps["shared"] = index.contains("/shared1/content.map")
                          ? inventory_shared_map(index.read("/shared1/content.map", 1024 * 1024))
                          : QJsonObject{{"present", false}, {"records", QJsonArray()}};
    return QJsonObject{{"entries", entries}, {"maps", maps}, {"titles", titles}};
  }

  auto verify_metadata(NandBin& nand, const QString& area) -> MetadataSummary {
    ImageIndex index(nand);
    return verify_expected_metadata(index, area);
  }

  void verify_stock_titles(NandBin& nand, const QMap<quint64, quint16>& titles) {
    ImageIndex index(nand);
    auto shared = read_shared_map(index);
    for (auto title = titles.cbegin(); title != titles.cend(); ++title) {
      const auto verified = verify_stock_title(index, shared, title.key(), title.value());
      if (verified.ios) require(titles.contains(verified.ios), "Stock title dependency is missing");
    }
  }

  auto verify_filesystem(NandBin& nand, bool generated, quint16 active_metadata) -> FilesystemSummary {
    const auto fats = nand.GetFats();
    require(fats.size() == 0x8000, "Invalid NAND allocation table");
    FilesystemSummary result;
    for (int cluster = 0x40; cluster < 0x7f00; ++cluster) {
      if (fats[cluster] == 0xfffd || fats[cluster] == 0xfffe) continue;
      for (int page = 0; page < 8; ++page)
        require(nand.CheckEcc(cluster * 8 + page), "Filesystem ECC check failed");
      ++result.checked_clusters;
    }
    require(active_metadata >= 0x7f00 && active_metadata <= 0x7ff0 && active_metadata % 16 == 0,
            "Invalid active metadata address");
    if (generated) {
      for (int cluster = 0x7f00; cluster < 0x8000; cluster += 16) {
        for (int page = 0; page < 16 * 8; ++page)
          require(nand.CheckEcc(cluster * 8 + page), "Generated metadata ECC check failed");
        result.checked_clusters += 16;
        require(nand.CheckHmacMeta(cluster), "Generated metadata authentication failed");
        ++result.authenticated_metadata_copies;
      }
    } else {
      for (int page = 0; page < 16 * 8; ++page)
        require(nand.CheckEcc(active_metadata * 8 + page), "Active metadata ECC check failed");
      result.checked_clusters += 16;
      for (int cluster = 0x7f00; cluster < 0x8000; cluster += 16)
        if (nand.CheckHmacMeta(cluster)) ++result.authenticated_metadata_copies;
      require(nand.CheckHmacMeta(active_metadata), "Active metadata authentication failed");
    }
    require(result.authenticated_metadata_copies > 0, "No filesystem metadata authenticated");

    ImageIndex index(nand);
    for (auto it = index.files().cbegin(); it != index.files().cend(); ++it) {
      const auto& entry = it.value();
      if (entry.directory) continue;
      require(nand.CheckHmacData(entry.id), "Filesystem file authentication failed");
      ++result.checked_files;
    }
    return result;
  }

  auto verify_stock(NandBin& nand, const QString& area) -> StockSummary {
    ImageIndex index(nand);
    const auto profile = factory::stock_titles(area);
    QSet<quint64> expected;
    for (auto it = profile.cbegin(); it != profile.cend(); ++it) expected.insert(it.key());
    require(!expected.isEmpty() && collect_title_ids(index) == expected,
            "Installed title set does not match the pinned stock profile");
    require(collect_ticket_ids(index) == expected,
            "Ticket set does not match the pinned stock profile");

    const auto uid_data = index.read("/sys/uid.sys", 1024 * 1024);
    UIDmap uid(uid_data);
    require(!uid_data.isEmpty() && uid.Check(), "Invalid UID map");
    UIDmap expected_uid;
    for (auto it = profile.cbegin(); it != profile.cend(); ++it) {
      require(expected_uid.GetUid(it.key()) != 0, "Cannot construct expected stock UID map");
    }
    require(expected_uid.Check() && uid.Data() == expected_uid.Data(),
            "UID map does not exactly match stock title assignments");
    require(index.read("/sys/cert.sys", CERTS_DAT_SIZE) ==
                QByteArray(reinterpret_cast<const char*>(certs_dat), CERTS_DAT_SIZE),
            "System certificate file differs from the generated stock layout");

    auto shared = read_shared_map(index);
    QSet<QString> expected_shared_files;
    QSet<QByteArray> expected_shared_hashes;
    QSet<QString> expected_directories{"/sys", "/ticket", "/title", "/shared1", "/shared2",
                                       "/shared2/sys", "/import", "/meta", "/tmp"};
    QSet<QString> expected_files{"/sys/cert.sys", "/sys/uid.sys", "/shared1/content.map",
                                 "/title/00000001/00000002/data/setting.txt"};
    StockSummary result;
    for (auto profile_item = profile.cbegin(); profile_item != profile.cend(); ++profile_item) {
      const auto id = profile_item.key();
      const auto id_text = title_path(id);
      const auto base = "/title/" + id_text.left(8) + "/" + id_text.mid(8);
      const auto ticket_dir = "/ticket/" + id_text.left(8);
      const auto content_dir = base + "/content";
      expected_directories.insert(ticket_dir);
      expected_directories.insert("/title/" + id_text.left(8));
      expected_directories.insert(base);
      expected_directories.insert(base + "/data");
      expected_directories.insert(content_dir);
      expected_files.insert(ticket_dir + "/" + id_text.mid(8) + ".tik");
      const auto verified = verify_stock_title(index, shared, id, profile_item.value());
      if (verified.ios) require(expected.contains(verified.ios), "Stock title dependency is missing");
      for (const auto& name : verified.local_names) expected_files.insert(content_dir + "/" + name);
      for (auto shared_app = verified.shared_apps.cbegin(); shared_app != verified.shared_apps.cend(); ++shared_app) {
        expected_shared_files.insert(shared_app.key());
        expected_shared_hashes.insert(shared_app.value());
        expected_files.insert("/shared1/" + shared_app.key());
      }
      result.checked_contents += verified.content_count;
      ++result.checked_titles;
    }

    QSet<QString> actual_shared_files;
    for (const auto& path : index.children("/shared1")) {
      const auto* entry = index.entry(path);
      require(entry && !entry->directory, "Unexpected shared content directory");
      actual_shared_files.insert(path.mid(QString("/shared1/").size()));
    }
    expected_shared_files.insert("content.map");
    require(actual_shared_files == expected_shared_files,
            "Shared content files do not match installed stock titles");

    require(shared.Count() == expected_shared_hashes.size(),
            "Shared content map contains unused entries");
    QSet<QString> expected_paths = expected_directories;
    expected_paths.unite(expected_files);
    QSet<QString> actual_paths;
    for (auto it = index.files().cbegin(); it != index.files().cend(); ++it)
      actual_paths.insert(it.key());
    require(actual_paths == expected_paths,
            "NAND paths do not match the generated stock layout");
    for (const auto& path : expected_directories)
      require(index.entry(path)->directory, "Expected stock directory has a file entry");
    for (const auto& path : expected_files)
      require(!index.entry(path)->directory, "Expected stock file has a directory entry");
    return result;
  }
} // namespace inspect
