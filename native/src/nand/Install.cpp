// SPDX-License-Identifier: GPL-2.0-only
#include <Install.hpp>

#include <Factory.hpp>
#include <Titles.hpp>
#include <WadPackage.hpp>
#include <WorkingImage.hpp>
#include <tiktmd.h>

#include <QTreeWidgetItem>
#include <algorithm>
#include <limits>
#include <memory>
#include <stdexcept>
#include <utility>

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
                    quint64& required_clusters, int& required_entries) {
    require(!path_exists(tree, path), "The target title already contains a file");
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

  void plan_and_install(NandBin& nand, int orphan_entries, NusJob job) {
    require(job.tid != 0 && job.version != TITLE_LATEST_VERSION,
            "A concrete title ID/version is required");
    require(decrypt_title(job), "Title metadata or content hash validation failed");

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

    // A fresh create may pre-create the system-menu data directory and settings
    // file. Keep that skeleton, but never let install_title's legacy replace
    // behavior delete a populated title content directory.
    require(!path_exists(tree.get(), ticket_path) && !path_exists(tree.get(), tmd_path),
            "The requested title is already installed");
    if (auto* existing_content = find_path(tree.get(), content_dir)) {
      require(entry_kind(existing_content) == NAND_DIR && existing_content->childCount() == 0,
              "The requested title has existing content files");
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
    add_new_file(tree.get(), ticket_path, ticket.Data().size(), needed_clusters, needed_entries);
    add_new_file(tree.get(), tmd_path, tmd.Data().size(), needed_clusters, needed_entries);

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
          require(quint64(existing_bytes.size()) == size && GetSha1(existing_bytes) == hash,
                  "Shared-content map references invalid NAND content");
          continue;
        }
        if (!planned_shared.GetAppFromHash(hash).isEmpty()) continue;
        const auto cid = planned_shared.GetNextEmptyCid();
        const auto path = shared_path(cid);
        require(!path_exists(tree.get(), path), "An unindexed shared-content path already exists");
        planned_shared.AddEntry(cid, hash);
        adds_shared = true;
        add_new_file(tree.get(), path, size, needed_clusters, needed_entries);
      } else {
        const auto path = content_dir + "/" + tmd.Cid(i) + ".app";
        add_new_file(tree.get(), path, size, needed_clusters, needed_entries);
      }
    }

    if (owner != old_owner) {
      add_resize(tree.get(), nand, "/sys/uid.sys", uid.Data().size(), needed_clusters, needed_entries);
    }
    if (adds_shared) {
      add_resize(tree.get(), nand, map_path, planned_shared.Data().size(), needed_clusters, needed_entries);
    }

    working_image::require_capacity(nand, root, orphan_entries, needed_entries, needed_clusters);

    // Existing UID/shared records are persisted in the same operation so the
    // next invocation can reopen the image and assign deterministic IDs.
    require(install_title(nand, uid, shared, job), "WiiQt failed while writing title files");
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

  void install_job_to_image(const QString& image, NusJob job) {
    working_image::Image working(image);
    plan_and_install(working.nand, working.layout.orphan_entries, std::move(job));
  }
} // namespace

namespace title_install {
  void install_nus_title(const QString& image, const QString& cache, quint64 title_id,
                         quint16 version, bool offline) {
    const auto job = factory::read_title(cache, title_id, version, offline);
    install_job_to_image(image, job);
  }

  void install_wad(const QString& image, const QString& package, const QByteArray& expected_sha256,
                   bool legacy) {
    require(!expected_sha256.isEmpty(), "An expected WAD SHA-256 is required for install");
    auto wad = wad_package::read(package, expected_sha256, legacy ? wad_package::Policy::LegacyChannel :
                                                                   wad_package::Policy::Standard);
    require(legacy || wad.nintendo_signed(), "WAD ticket and TMD must be genuinely Nintendo-signed");
    const auto type = wad.job.tid >> 32;
    require(type == 0x00010001 || type == 0x00010002, "WAD install supports downloadable and system channels");
    install_job_to_image(image, std::move(wad.job));
  }

  namespace detail {
    void install_validated_job(const QString& image, NusJob job) {
      install_job_to_image(image, std::move(job));
    }
  } // namespace detail
} // namespace title_install
