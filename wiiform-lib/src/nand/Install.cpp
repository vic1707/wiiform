// SPDX-License-Identifier: GPL-2.0-only
#include <Install.hpp>

#include <Factory.hpp>
#include <Layout.hpp>
#include <Titles.hpp>
#include <WadPackage.hpp>
#include <WorkingImage.hpp>
#include <tiktmd.h>
#include <sharedcontentmap.h>

#include <QTreeWidgetItem>
#include <QSet>
#include <QtEndian>
#include <algorithm>
#include <limits>
#include <memory>
#include <stdexcept>

namespace {
  using working_image::entry_id;
  using working_image::entry_kind;
  using working_image::find_path;
  using working_image::read_authenticated_file;

  constexpr quint64 cluster_size = 0x4000;

  void require(bool condition, const QString& message) {
    if (!condition) throw std::runtime_error(message.toStdString());
  }

  auto title_name(quint64 tid) -> QString {
    return QString("%1").arg(tid, 16, 16, QChar('0'));
  }

  auto path_exists(QTreeWidgetItem* tree, const QString& path) -> bool {
    return find_path(tree, path) != nullptr;
  }

  void require_directory(QTreeWidgetItem* tree, const QString& path) {
    if (auto* item = find_path(tree, path))
      require(entry_kind(item) == NAND_DIR, "A title path component is not a directory");
  }

  auto clusters_for(quint64 bytes) -> quint64 {
    require(bytes > 0 && bytes <= std::numeric_limits<quint32>::max(),
            "Empty or oversized title file is unsupported");
    return (bytes + cluster_size - 1) / cluster_size;
  }

  auto existing_file_clusters(NandBin& nand, QTreeWidgetItem* tree, const QString& path) -> quint64 {
    const auto* item = find_path(tree, path);
    require(item && entry_kind(item) == NAND_FILE, "Expected NAND file is missing or has the wrong type");
    return static_cast<quint64>(nand.GetFatsForFile(entry_id(item)).size());
  }

  void add_new_entry(QTreeWidgetItem* tree, const QString& path, int& required_entries) {
    if (path_exists(tree, path)) return;
    ++required_entries;
  }

  void add_new_file(QTreeWidgetItem* tree, const QString& path, quint64 bytes,
                    quint64& required_clusters, int& required_entries,
                    const QSet<QString>& replaced_files = {}) {
    require(!path_exists(tree, path) || replaced_files.contains(path),
            "The target title already contains a file");
    required_clusters += clusters_for(bytes);
    ++required_entries;
  }

  void add_resize(QTreeWidgetItem* tree, NandBin& nand, const QString& path, quint64 bytes,
                  quint64& required_clusters, int& required_entries) {
    const auto* item = find_path(tree, path);
    if (!item) {
      required_clusters += clusters_for(bytes);
      ++required_entries;
      return;
    }
    require(entry_kind(item) == NAND_FILE, "NAND map path has the wrong type");
    const auto old_clusters = existing_file_clusters(nand, tree, path);
    const auto new_clusters = clusters_for(bytes);
    if (new_clusters > old_clusters) required_clusters += new_clusters - old_clusters;
  }

  auto shared_path(const QString& cid) -> QString {
    return "/shared1/" + cid + ".app";
  }

  auto installed_wad_matches(NandBin& nand, QTreeWidgetItem* tree, const QString& content_dir,
                            const QString& ticket_path, const ValidatedTitleJob& validated) -> bool {
    const auto& job = validated.get();
    const auto tmd_path = content_dir + "/title.tmd";
    const auto* tmd_entry = find_path(tree, tmd_path);
    const auto* ticket_entry = find_path(tree, ticket_path);
    require(tmd_entry && ticket_entry,
            "Installed title is incomplete or inconsistent with WAD");

    constexpr quint64 max_tmd_bytes = 0x1e4 + 0xffff * 0x24;
    const auto installed_tmd = read_authenticated_file(nand, tmd_entry, max_tmd_bytes);
    const auto installed_ticket = read_authenticated_file(nand, ticket_entry, 64 * 1024);
    if (installed_tmd != job.data[0] || installed_ticket != job.data[1]) return false;

    const auto map_path = QString("/shared1/content.map");
    const auto* map_entry = find_path(tree, map_path);
    const auto map_data = map_entry
        ? read_authenticated_file(nand, map_entry, 1024 * 1024)
        : QByteArray();
    SharedContentMap shared(map_data);
    require(map_data.isEmpty() || shared.Check(),
            "Installed title is incomplete or inconsistent with WAD");

    Tmd tmd(installed_tmd);
    QSet<QString> expected_local_files;
    for (int index = 0; index < tmd.Count(); ++index) {
      const auto expected_content = job.data[index + 2];
      const auto cid = tmd.Cid(index);
      QString content_path;
      if (tmd.Type(index) == 0x8001) {
        const auto shared_cid = shared.GetAppFromHash(tmd.Hash(index));
        require(!shared_cid.isEmpty(),
                "Installed title is incomplete or inconsistent with WAD");
        content_path = shared_path(shared_cid);
      } else {
        content_path = content_dir + "/" + cid + ".app";
        expected_local_files.insert(cid + ".app");
      }
      const auto* content_file = find_path(tree, content_path);
      require(content_file, "Installed title is incomplete or inconsistent with WAD");
      const auto installed_content = read_authenticated_file(nand, content_file, expected_content.size());
      if (installed_content != expected_content)
        throw std::runtime_error("Installed title content conflicts with WAD");
    }

    const auto* content_entry = find_path(tree, content_dir);
    require(content_entry && entry_kind(content_entry) == NAND_DIR,
            "Installed title is incomplete or inconsistent with WAD");
    QSet<QString> actual_local_files;
    for (int index = 0; index < content_entry->childCount(); ++index) {
      const auto* child = content_entry->child(index);
      require(entry_kind(child) == NAND_FILE,
              "Installed title is incomplete or inconsistent with WAD");
      const auto name = child->text(0);
      if (name != "title.tmd") actual_local_files.insert(name);
    }
    require(actual_local_files == expected_local_files,
            "Installed title is incomplete or inconsistent with WAD");
    return true;
  }

  void verify_stock_stub(NandBin& nand, QTreeWidgetItem* tree, quint64 title_id) {
    const auto tid = title_name(title_id);
    const auto title = "/title/" + tid.left(8) + "/" + tid.mid(8);
    const auto content_dir = title + "/content";
    const auto tmd_path = content_dir + "/title.tmd";
    const auto ticket_path = "/ticket/" + tid.left(8) + "/" + tid.mid(8) + ".tik";
    const auto* tmd_entry = find_path(tree, tmd_path);
    const auto* ticket_entry = find_path(tree, ticket_path);
    require(tmd_entry && ticket_entry, "The occupied cIOS slot is not a complete stock stub");
    const auto metadata = read_authenticated_file(nand, tmd_entry, 2 * 1024 * 1024);
    const auto ticket_data = read_authenticated_file(nand, ticket_entry, 64 * 1024);
    require(check_title_metadata(metadata, ticket_data) &&
                check_cert_chain(metadata) == ERROR_SUCCESS && check_cert_chain(ticket_data) == ERROR_SUCCESS,
            "The occupied cIOS slot is not an authentic Nintendo stub");
    const auto count = qFromBigEndian<quint16>(metadata.constData() + 0x1de);
    require(metadata.size() == 0x1e4 + count * 0x24 && ticket_data.size() == 0x2a4 && count > 0,
            "The occupied cIOS slot has unsupported stock metadata sizes");
    require(metadata[0x183] == '\0' && qFromBigEndian<quint32>(ticket_data.constData() + 0x1d8) == 0 &&
                static_cast<uchar>(ticket_data[0x1f1]) == 0,
            "The occupied cIOS slot is not a universal-key Wii title");
    Tmd tmd(metadata);
    Ticket ticket(ticket_data, false);
    require(tmd.Tid() == title_id && ticket.Tid() == title_id && tmd.Version() == 65280,
            "The occupied cIOS slot is not Nintendo stub v65280");

    const auto* map_entry = find_path(tree, "/shared1/content.map");
    const auto map_data = map_entry
        ? read_authenticated_file(nand, map_entry, 1024 * 1024)
        : QByteArray();
    SharedContentMap shared(map_data);
    require(map_data.isEmpty() || shared.Check(), "The NAND shared-content map is malformed");

    NusJob job;
    job.tid = title_id;
    job.version = 65280;
    job.decrypt = true;
    job.data << metadata << ticket_data;
    QSet<QString> expected_local_files{"title.tmd"};
    quint64 total_size = 0;
    for (int index = 0; index < tmd.Count(); ++index) {
      const auto size = tmd.Size(index);
      require(size > 0 && size <= 0x20000000 - total_size,
              "The occupied cIOS slot has invalid stub content");
      total_size += size;
      QString path;
      if (tmd.Type(index) == 0x8001) {
        const auto cid = shared.GetAppFromHash(tmd.Hash(index));
        require(!cid.isEmpty(), "The occupied cIOS slot has missing shared content");
        path = shared_path(cid);
      } else {
        require(tmd.Type(index) == 1, "The occupied cIOS slot has unsupported stub content");
        const auto cid = tmd.Cid(index);
        expected_local_files.insert(cid + ".app");
        path = content_dir + "/" + cid + ".app";
      }
      const auto* app_entry = find_path(tree, path);
      require(app_entry, "The occupied cIOS slot has missing stub content");
      const auto app = read_authenticated_file(nand, app_entry, size);
      require(quint64(app.size()) == size, "The occupied cIOS slot has incomplete stub content");
      job.data << app;
    }
    const auto* content_entry = find_path(tree, content_dir);
    require(content_entry && entry_kind(content_entry) == NAND_DIR,
            "The occupied cIOS slot has no content directory");
    QSet<QString> actual_files;
    for (int index = 0; index < content_entry->childCount(); ++index) {
      const auto* child = content_entry->child(index);
      require(entry_kind(child) == NAND_FILE,
              "The occupied cIOS slot has unexpected content entries");
      actual_files.insert(child->text(0));
    }
    require(actual_files == expected_local_files && decrypt_title(job),
            "The occupied cIOS slot does not contain the authentic Nintendo stub");
  }

  void plan_and_install(NandBin& nand, int orphan_entries, const ValidatedTitleJob& validated,
                        const QSet<QString>& replaced_files = {}) {
    const auto& job = validated.get();
    require(job.tid != 0, "A concrete title ID is required");

    Tmd tmd(job.data[0]);
    Ticket ticket(job.data[1], false);
    const auto tid = title_name(job.tid);
    const auto high = tid.left(8);
    const auto low = tid.mid(8);
    const auto title = "/title/" + high + "/" + low;
    const auto ticket_dir = "/ticket/" + high;
    const auto ticket_path = ticket_dir + "/" + low + ".tik";
    const auto content_dir = title + "/content";
    const auto tmd_path = content_dir + "/title.tmd";

    require(ticket.Tid() == job.tid && tmd.Tid() == job.tid && tmd.Version() == job.version,
            "Title ID/version does not match its metadata");

    std::unique_ptr<QTreeWidgetItem> tree(nand.GetTree());
    require(tree && tree->childCount() == 1, "Cannot inspect NAND filesystem tree");
    auto* root = tree->child(0);
    require(root && root->text(0) == "/", "Cannot inspect NAND filesystem root");

    // A fresh stock image may already contain the title's directory skeleton,
    // including system-menu settings. Preserve NUS installation semantics by
    // rejecting only the same completed-title markers as before.
    require((!path_exists(tree.get(), ticket_path) || replaced_files.contains(ticket_path)) &&
                (!path_exists(tree.get(), tmd_path) || replaced_files.contains(tmd_path)),
            "The requested title is already installed");
    if (auto* existing_content = find_path(tree.get(), content_dir)) {
      require(entry_kind(existing_content) == NAND_DIR,
              "The requested title content path is not a directory");
      for (int i = 0; i < existing_content->childCount(); ++i) {
        const auto* child = existing_content->child(i);
        require(replaced_files.contains(content_dir + "/" + child->text(0)),
                "The requested title has existing content files");
      }
    }
    require_directory(tree.get(), ticket_dir);
    require_directory(tree.get(), "/ticket");
    require_directory(tree.get(), "/title");
    require_directory(tree.get(), "/shared1");
    require_directory(tree.get(), "/sys");
    require_directory(tree.get(), "/title/" + high);
    require_directory(tree.get(), title);
    require_directory(tree.get(), title + "/data");
    require_directory(tree.get(), content_dir);

    const auto uid_data = read_authenticated_file(nand, find_path(tree.get(), "/sys/uid.sys"), 1024 * 1024);
    require(!uid_data.isEmpty(), "NAND UID map is empty");
    UIDmap uid(uid_data);
    require(uid.Check(), "Existing NAND UID map is malformed");
    const auto old_owner = uid.GetUid(job.tid, false);
    const auto owner = uid.GetUid(job.tid);
    require(owner != 0 && uid.Check(), "Cannot assign a valid title UID");

    const auto map_path = QString("/shared1/content.map");
    const auto* existing_map = find_path(tree.get(), map_path);
    const auto map_data = existing_map ? read_authenticated_file(nand, existing_map, 1024 * 1024) : QByteArray();
    SharedContentMap shared(map_data);
    require(map_data.isEmpty() || shared.Check(), "Existing shared-content map is malformed");
    SharedContentMap planned_shared(map_data);

    int needed_entries = 0;
    quint64 needed_clusters = 0;
    add_new_entry(tree.get(), ticket_dir, needed_entries);
    add_new_entry(tree.get(), "/title/" + high, needed_entries);
    add_new_entry(tree.get(), title, needed_entries);
    add_new_entry(tree.get(), title + "/data", needed_entries);
    add_new_entry(tree.get(), content_dir, needed_entries);
    add_new_file(tree.get(), ticket_path, ticket.Data().size(), needed_clusters, needed_entries, replaced_files);
    add_new_file(tree.get(), tmd_path, tmd.Data().size(), needed_clusters, needed_entries, replaced_files);

    bool adds_shared = false;
    for (int i = 0; i < tmd.Count(); ++i) {
      const auto size = tmd.Size(i);
      require(size > 0 && quint64(job.data[i + 2].size()) == size,
              "Empty or inconsistent title content is unsupported");
      if (tmd.Type(i) == 0x8001) {
        const auto hash = tmd.Hash(i);
        const auto existing_cid = shared.GetAppFromHash(hash);
        if (!existing_cid.isEmpty()) {
          const auto existing_bytes = read_authenticated_file(nand, find_path(tree.get(), shared_path(existing_cid)), size);
          require(existing_bytes == job.data[i + 2],
                  "Shared-content map references invalid NAND content");
          continue;
        }
        if (!planned_shared.GetAppFromHash(hash).isEmpty()) continue;
        const auto cid = planned_shared.GetNextEmptyCid();
        const auto path = shared_path(cid);
        require(!path_exists(tree.get(), path), "An unindexed shared-content path already exists");
        planned_shared.AddEntry(cid, hash);
        adds_shared = true;
        add_new_file(tree.get(), path, size, needed_clusters, needed_entries, replaced_files);
      } else {
        const auto path = content_dir + "/" + tmd.Cid(i) + ".app";
        add_new_file(tree.get(), path, size, needed_clusters, needed_entries, replaced_files);
      }
    }

    if (owner != old_owner) {
      add_resize(tree.get(), nand, "/sys/uid.sys", uid.Data().size(), needed_clusters, needed_entries);
    }
    if (adds_shared) {
      add_resize(tree.get(), nand, map_path, planned_shared.Data().size(), needed_clusters, needed_entries);
    }

    quint64 released_clusters = 0;
    int released_entries = 0;
    for (const auto& path : replaced_files) {
      const auto* entry = find_path(tree.get(), path);
      if (!entry) continue;
      require(entry_kind(entry) == NAND_FILE, "A cIOS replacement path is not a file");
      released_clusters += existing_file_clusters(nand, tree.get(), path);
      ++released_entries;
    }
    working_image::require_capacity(nand, root, orphan_entries, needed_entries, needed_clusters,
                                    released_entries, released_clusters);
    if (!replaced_files.isEmpty()) {
      for (const auto& path : replaced_files) {
        if (find_path(tree.get(), path))
          require(nand.Delete(path), "Cannot remove the original stock stub file");
      }
    }

    // Existing UID/shared records are persisted in the same operation so the
    // next invocation can reopen the image and assign deterministic IDs.
    require(install_title(nand, uid, shared, validated), "WiiQt failed while writing title files");
    if (owner != old_owner)
      require(nand.SetData("/sys/uid.sys", uid.Data()), "Cannot persist NAND UID map");
    if (adds_shared) {
      auto* entry = find_path(tree.get(), map_path);
      quint16 index = 0;
      if (!entry) index = nand.CreateEntry(map_path, 0, 0, NAND_FILE, NAND_RW, NAND_RW, 0);
      else index = entry_id(entry);
      require(index && nand.SetData(index, shared.Data()), "Cannot persist shared-content map");
    }
    require(nand.WriteMetaData(), "Cannot persist NAND metadata");
  }

  auto install_validated_wad(std::span<std::uint8_t> image, const ValidatedTitleJob& job,
                             bool replace_stock_stub) -> wiiform::InstalledWad {
    const auto& parsed = job.get();
    const auto title_id = parsed.tid;
    const auto region = qFromBigEndian<quint16>(parsed.data[0].constData() + 0x19c);
    const auto version = parsed.version;
    const auto tid = title_name(title_id);
    const auto title = "/title/" + tid.left(8) + "/" + tid.mid(8);
    const auto content_dir = title + "/content";
    const auto ticket_path = "/ticket/" + tid.left(8) + "/" + tid.mid(8) + ".tik";
    QSet<QString> replaced_files;
    {
      working_image::Image working(image);
      std::unique_ptr<QTreeWidgetItem> tree(working.nand.GetTree());
      require(tree && tree->childCount() == 1 && tree->child(0) && tree->child(0)->text(0) == "/",
              "Cannot inspect NAND filesystem tree");
      const auto* content = find_path(tree.get(), content_dir);
      const bool occupied = path_exists(tree.get(), ticket_path) ||
          (content && content->childCount() != 0);
      if (occupied) {
        if (installed_wad_matches(working.nand, tree.get(), content_dir, ticket_path, job))
          return {title_id, region, version, true};
        require(replace_stock_stub, "Installed title conflicts with WAD title metadata");

        const auto slot = static_cast<quint32>(title_id);
        require((title_id >> 32) == 1 && (slot == 249 || slot == 250),
                "Stock stub replacement is supported only in slots 249 and 250");
        verify_stock_stub(working.nand, tree.get(), title_id);
        const auto* current_ticket = find_path(tree.get(), ticket_path);
        require(current_ticket && entry_kind(current_ticket) == NAND_FILE,
                "The occupied cIOS slot has no valid ticket file");
        replaced_files.insert(ticket_path);
        const auto* current_content = find_path(tree.get(), content_dir);
        require(current_content && entry_kind(current_content) == NAND_DIR,
                "The occupied cIOS slot has no content directory");
        for (int index = 0; index < current_content->childCount(); ++index) {
          const auto* child = current_content->child(index);
          require(entry_kind(child) == NAND_FILE,
                  "The occupied cIOS slot has unexpected content entries");
          replaced_files.insert(content_dir + "/" + child->text(0));
        }
        const auto uid_bytes = read_authenticated_file(working.nand, find_path(tree.get(), "/sys/uid.sys"),
                                                        1024 * 1024);
        UIDmap uid(uid_bytes);
        require(uid.Check() && uid.GetUid(title_id, false) != 0,
                "The authentic stock stub has no matching UID record");
      }
      plan_and_install(working.nand, working.layout.orphan_entries, job, replaced_files);
    }

    working_image::Image readback(image);
    std::unique_ptr<QTreeWidgetItem> tree(readback.nand.GetTree());
    require(tree && tree->childCount() == 1 && tree->child(0) && tree->child(0)->text(0) == "/",
            "Cannot inspect NAND filesystem after WAD install");
    require(installed_wad_matches(readback.nand, tree.get(), content_dir, ticket_path, job),
            "Installed title conflicts with WAD title metadata");
    return {title_id, region, version, false};
  }

  auto read_title_from_nand(NandBin& nand, QTreeWidgetItem* tree, quint64 title_id,
                            quint16 version, const char* unavailable_message) -> NusJob {
    constexpr quint64 max_tmd_size = 0x1e4 + 0xffff * 0x24;
    const auto id = title_name(title_id);
    const auto title = "/title/" + id.left(8) + "/" + id.mid(8);
    const auto content_dir = title + "/content";
    const auto ticket_path = "/ticket/" + id.left(8) + "/" + id.mid(8) + ".tik";

    const auto* tmd_entry = find_path(tree, content_dir + "/title.tmd");
    const auto* ticket_entry = find_path(tree, ticket_path);
    require(tmd_entry && ticket_entry, unavailable_message);
    const auto tmd = read_authenticated_file(nand, tmd_entry, max_tmd_size);
    const auto ticket_bytes = read_authenticated_file(nand, ticket_entry, 0x2a4);
    require(check_title_metadata(tmd, ticket_bytes) && ticket_bytes.size() == 0x2a4,
            "The NAND title ticket has an invalid size or structure");
    Tmd metadata(tmd);
    Ticket ticket(ticket_bytes, false);
    require(tmd.size() == 0x1e4 + metadata.Count() * 0x24,
            "The NAND title metadata size is invalid");
    require(metadata.Tid() == title_id && metadata.Version() == version,
            unavailable_message);
    require(ticket.Tid() == title_id, "The NAND title ticket does not match its metadata");

    const auto* map_entry = find_path(tree, "/shared1/content.map");
    const auto map = map_entry ? read_authenticated_file(nand, map_entry, 1024 * 1024) : QByteArray();
    SharedContentMap shared(map);
    require(map.isEmpty() || shared.Check(), "The NAND shared-content map is invalid");

    NusJob job;
    job.tid = title_id;
    job.version = version;
    job.decrypt = true;
    job.data << tmd << ticket_bytes;
    quint64 total_size = 0;
    QSet<QString> expected_files{"title.tmd"};
    for (int index = 0; index < metadata.Count(); ++index) {
      const auto size = metadata.Size(index);
      require(size > 0 && size <= 0x20000000 - total_size,
              "The NAND title has invalid content sizes");
      total_size += size;
      QString path;
      if (metadata.Type(index) == 0x8001) {
        const auto cid = shared.GetAppFromHash(metadata.Hash(index));
        require(!cid.isEmpty(), "The NAND title is missing a shared content file");
        path = shared_path(cid);
      } else {
        require(metadata.Type(index) == 1, "The NAND title has an unsupported content type");
        const auto cid = metadata.Cid(index);
        expected_files.insert(cid + ".app");
        path = content_dir + "/" + cid + ".app";
      }
      job.data << read_authenticated_file(nand, find_path(tree, path), size);
    }

    const auto* content = find_path(tree, content_dir);
    require(content && entry_kind(content) == NAND_DIR, "The NAND title has no content directory");
    QSet<QString> actual_files;
    for (int index = 0; index < content->childCount(); ++index) {
      const auto* entry = content->child(index);
      require(entry_kind(entry) == NAND_FILE, "The NAND title has an unexpected content entry");
      actual_files.insert(entry->text(0));
    }
    require(actual_files == expected_files, "The NAND title content does not match its TMD");
    return job;
  }
} // namespace

namespace title_install {
  void install_nus_title(std::span<std::uint8_t> image, const QString& cache, quint64 title_id,
                         quint16 version, bool offline) {
    const auto job = factory::read_title(cache, title_id, version, offline);
    working_image::Image working(image);
    plan_and_install(working.nand, working.layout.orphan_entries, job);
  }

  auto install_wad(std::span<std::uint8_t> image, std::span<const std::uint8_t> package,
                   const wiiform::Sha256& expected_sha256, wiiform::WadPolicy policy) -> wiiform::InstalledWad {
    const auto job = wad_package::read(package, expected_sha256, policy);
    return install_validated_wad(image, job, false);
  }

  auto install_cios(std::span<std::uint8_t> image, std::span<const std::uint8_t> package,
                    const wiiform::Sha256& expected_sha256,
                    const wiiform::CiosOptions& options) -> wiiform::InstalledWad {
    const auto job = wad_package::read_cios(package, expected_sha256, options);
    const auto& parsed = job.get();
    const auto expected_id = (quint64(1) << 32) | options.slot;
    require(parsed.tid == expected_id && parsed.version == options.revision,
            "cIOS WAD does not match its declared slot and revision");
    return install_validated_wad(image, job, options.replace_stock_stub);
  }

  auto preserve_homebrew_channel(std::span<std::uint8_t> image,
                                 std::span<const std::uint8_t> backup) -> wiiform::InstalledWad {
    constexpr quint64 hbc_id = 0x000100014c554c5aull;
    constexpr quint16 hbc_version = 258;
    constexpr quint64 ios58_id = 0x000000010000003aull;
    constexpr quint16 ios58_version = 6176;
    constexpr std::size_t full_size = 0x21000400;
    constexpr std::size_t footer_size = 0x400;
    constexpr std::size_t console_identity_offset = 0x124;
    constexpr std::size_t console_identity_size = 0x44;
    require(image.size() == full_size && backup.size() == full_size,
            "Expected footer-bearing destination and source NAND images");

    const auto source_identity = backup.last(footer_size).subspan(console_identity_offset,
                                                                  console_identity_size);
    const auto target_identity = std::span<const std::uint8_t>(image).last(footer_size).subspan(
        console_identity_offset, console_identity_size);
    require(std::equal(source_identity.begin(), source_identity.end(), target_identity.begin()),
            "The HBC backup belongs to a different Wii");

    const auto source_layout = inspect::validate_layout(backup, false);
    nand_buffer::Device source_device{backup};
    NandBin source;
    require(source.SetDevice(&source_device) && source.InitNand() &&
                source.CheckHmacMeta(source_layout.active_metadata),
            "Cannot authenticate the HBC source backup");
    std::unique_ptr<QTreeWidgetItem> source_tree(source.GetTree());
    require(source_tree && source_tree->childCount() == 1,
            "Cannot read the HBC source backup filesystem");
    const auto hbc = validate_title(read_title_from_nand(
                                        source, source_tree.get(), hbc_id, hbc_version,
                                        "Backup does not contain original Homebrew Channel 1.1.2 (LULZ)"),
                                    "The Homebrew Channel backup content is invalid");
    const auto ios58 = validate_title(read_title_from_nand(
                                          source, source_tree.get(), ios58_id, ios58_version,
                                          "The HBC backup does not contain IOS58 v6176"),
                                      "The IOS58 backup content is invalid");
    require(check_cert_chain(ios58.get().data[0]) == ERROR_SUCCESS &&
                check_cert_chain(ios58.get().data[1]) == ERROR_SUCCESS,
            "The HBC backup does not contain Nintendo-signed IOS58 v6176");
    const auto source_console_id = qFromBigEndian<quint32>(
        reinterpret_cast<const uchar*>(backup.data() + backup.size() - footer_size + console_identity_offset));
    Tmd hbc_tmd(hbc.get().data[0]);
    require(hbc_tmd.IOS() == ios58_id &&
                (qFromBigEndian<quint32>(hbc.get().data[1].constData() + 0x1d8) == 0 ||
                 qFromBigEndian<quint32>(hbc.get().data[1].constData() + 0x1d8) == source_console_id) &&
                static_cast<uchar>(hbc.get().data[1][0x1f1]) == 0,
            "The backup does not contain the supported IOS58 Homebrew Channel release");

    {
      working_image::Image target(image);
      std::unique_ptr<QTreeWidgetItem> target_tree(target.nand.GetTree());
      require(target_tree && target_tree->childCount() == 1,
              "Cannot read the destination NAND filesystem");
      require(installed_wad_matches(target.nand, target_tree.get(),
                                    "/title/00000001/0000003a/content",
                                    "/ticket/00000001/0000003a.tik", ios58),
              "The destination NAND does not contain authentic stock IOS58 v6176");
    }
    return install_validated_wad(image, hbc, false);
  }

} // namespace title_install
