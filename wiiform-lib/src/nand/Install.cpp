// SPDX-License-Identifier: GPL-2.0-only
#include <Install.hpp>
#include <Support.hpp>

#include <Factory.hpp>
#include <CiosBuild.hpp>
#include <RvloaderIos.hpp>
#include <Titles.hpp>
#include <WadPackage.hpp>
#include <WorkingImage.hpp>
#include <tiktmd.h>

#include <QSet>
#include <QtEndian>
#include <algorithm>
#include <limits>
#include <optional>
#include <stdexcept>

namespace {
  using working_image::find_path;
  using working_image::read_authenticated_file;

  constexpr quint64 cluster_size = 0x4000;

  auto path_exists(NandBin& nand, const QString& path) -> bool {
    return nand.Stat(path);
  }

  void require_directory(NandBin& nand, const QString& path) {
    if (const auto item = find_path(nand, path))
      require(item->IsDir(), "A title path component is not a directory");
  }

  auto clusters_for(quint64 bytes) -> quint64 {
    require(bytes > 0 && bytes <= std::numeric_limits<quint32>::max(),
            "Empty or oversized title file is unsupported");
    return (bytes + cluster_size - 1) / cluster_size;
  }

  auto existing_file_clusters(NandBin& nand, const QString& path) -> quint64 {
    const auto item = find_path(nand, path);
    require(item && item->IsFile(), "Expected NAND file is missing or has the wrong type");
    return static_cast<quint64>(nand.GetFatsForFile(item->index).size());
  }

  void add_new_entry(NandBin& nand, const QString& path, int& required_entries) {
    if (path_exists(nand, path)) return;
    ++required_entries;
  }

  void add_new_file(NandBin& nand, const QString& path, quint64 bytes,
                    quint64& required_clusters, int& required_entries,
                    const QSet<QString>& replaced_files = {}) {
    require(!path_exists(nand, path) || replaced_files.contains(path),
            "The target title already contains a file");
    required_clusters += clusters_for(bytes);
    ++required_entries;
  }

  void add_resize(NandBin& nand, const QString& path, quint64 bytes,
                  quint64& required_clusters, int& required_entries) {
    const auto item = find_path(nand, path);
    if (!item) {
      required_clusters += clusters_for(bytes);
      ++required_entries;
      return;
    }
    require(item->IsFile(), "NAND map path has the wrong type");
    const auto old_clusters = existing_file_clusters(nand, path);
    const auto new_clusters = clusters_for(bytes);
    if (new_clusters > old_clusters) required_clusters += new_clusters - old_clusters;
  }

  auto shared_path(const QString& cid) -> QString {
    return "/shared1/" + cid + ".app";
  }

  auto installed_wad_matches(NandBin& nand, const QString& content_dir,
                            const QString& ticket_path, const ValidatedTitleJob& validated) -> bool {
    const auto& job = validated.get();
    const auto tmd_path = content_dir + "/title.tmd";
    const auto tmd_entry = find_path(nand, tmd_path);
    const auto ticket_entry = find_path(nand, ticket_path);
    require(tmd_entry && ticket_entry,
            "Installed title is incomplete or inconsistent with WAD");

    constexpr quint64 max_tmd_bytes = 0x1e4 + 0xffff * 0x24;
    const auto installed_tmd = read_authenticated_file(nand, tmd_entry, max_tmd_bytes);
    const auto installed_ticket = read_authenticated_file(nand, ticket_entry, 64 * 1024);
    if (installed_tmd != job.data[0] || installed_ticket != job.data[1]) return false;

    const auto map_path = QString("/shared1/content.map");
    const auto map_entry = find_path(nand, map_path);
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
      const auto content_file = find_path(nand, content_path);
      require(content_file.has_value(), "Installed title is incomplete or inconsistent with WAD");
      const auto installed_content = read_authenticated_file(nand, content_file, expected_content.size());
      if (installed_content != expected_content)
        throw std::runtime_error("Installed title content conflicts with WAD");
    }

    const auto content_entry = find_path(nand, content_dir);
    require(content_entry && content_entry->IsDir(),
            "Installed title is incomplete or inconsistent with WAD");
    QSet<QString> actual_local_files;
    for (const auto& child : nand.Children(content_entry->index)) {
      require(child.IsFile(), "Installed title is incomplete or inconsistent with WAD");
      if (child.name != "title.tmd") actual_local_files.insert(child.name);
    }
    require(actual_local_files == expected_local_files,
            "Installed title is incomplete or inconsistent with WAD");
    return true;
  }

  // Checks that the slot holds exactly this Nintendo-signed title version, complete.
  void verify_nintendo_title(NandBin& nand, quint64 title_id, quint16 version) {
    const auto tid = title_name(title_id);
    const auto title = "/title/" + tid.left(8) + "/" + tid.mid(8);
    const auto content_dir = title + "/content";
    const auto tmd_path = content_dir + "/title.tmd";
    const auto ticket_path = "/ticket/" + tid.left(8) + "/" + tid.mid(8) + ".tik";
    const auto tmd_entry = find_path(nand, tmd_path);
    const auto ticket_entry = find_path(nand, ticket_path);
    require(tmd_entry && ticket_entry, "The occupied slot is not a complete Nintendo title");
    const auto metadata = read_authenticated_file(nand, tmd_entry, 2 * 1024 * 1024);
    const auto ticket_data = read_authenticated_file(nand, ticket_entry, 64 * 1024);
    require(CheckTitleMetadata(metadata, ticket_data) &&
                check_cert_chain(metadata) == ERROR_SUCCESS && check_cert_chain(ticket_data) == ERROR_SUCCESS,
            "The occupied slot is not an authentic Nintendo title");
    const auto count = qFromBigEndian<quint16>(metadata.constData() + 0x1de);
    require(metadata.size() == 0x1e4 + count * 0x24 && ticket_data.size() == 0x2a4 && count > 0,
            "The occupied slot has unsupported stock metadata sizes");
    require(metadata[0x183] == '\0' && qFromBigEndian<quint32>(ticket_data.constData() + 0x1d8) == 0 &&
                static_cast<uchar>(ticket_data[0x1f1]) == 0,
            "The occupied slot is not a universal-key Wii title");
    Tmd tmd(metadata);
    Ticket ticket(ticket_data, false);
    require(tmd.Tid() == title_id && ticket.Tid() == title_id && tmd.Version() == version,
            "The occupied slot is not the expected Nintendo title version");

    const auto map_entry = find_path(nand, "/shared1/content.map");
    const auto map_data = map_entry
        ? read_authenticated_file(nand, map_entry, 1024 * 1024)
        : QByteArray();
    SharedContentMap shared(map_data);
    require(map_data.isEmpty() || shared.Check(), "The NAND shared-content map is malformed");

    NusJob job;
    job.tid = title_id;
    job.version = version;
    job.decrypt = true;
    job.data << metadata << ticket_data;
    QSet<QString> expected_local_files{"title.tmd"};
    quint64 total_size = 0;
    for (int index = 0; index < tmd.Count(); ++index) {
      const auto size = tmd.Size(index);
      require(size > 0 && size <= 0x20000000 - total_size,
              "The occupied slot has invalid title content");
      total_size += size;
      QString path;
      if (tmd.Type(index) == 0x8001) {
        const auto cid = shared.GetAppFromHash(tmd.Hash(index));
        require(!cid.isEmpty(), "The occupied slot has missing shared content");
        path = shared_path(cid);
      } else {
        require(tmd.Type(index) == 1, "The occupied slot has unsupported title content");
        const auto cid = tmd.Cid(index);
        expected_local_files.insert(cid + ".app");
        path = content_dir + "/" + cid + ".app";
      }
      const auto app_entry = find_path(nand, path);
      require(app_entry.has_value(), "The occupied slot has missing title content");
      const auto app = read_authenticated_file(nand, app_entry, size);
      require(quint64(app.size()) == size, "The occupied slot has incomplete title content");
      job.data << app;
    }
    const auto content_entry = find_path(nand, content_dir);
    require(content_entry && content_entry->IsDir(),
            "The occupied slot has no content directory");
    QSet<QString> actual_files;
    for (const auto& child : nand.Children(content_entry->index)) {
      require(child.IsFile(), "The occupied slot has unexpected content entries");
      actual_files.insert(child.name);
    }
    require(actual_files == expected_local_files && decrypt_title(job),
            "The occupied slot does not contain the authentic Nintendo title");
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

    require(path_exists(nand, "/"), "Cannot inspect NAND filesystem root");

    // A fresh stock image may already contain the title's directory skeleton,
    // including system-menu settings. Preserve NUS installation semantics by
    // rejecting only the same completed-title markers as before.
    require((!path_exists(nand, ticket_path) || replaced_files.contains(ticket_path)) &&
                (!path_exists(nand, tmd_path) || replaced_files.contains(tmd_path)),
            "The requested title is already installed");
    if (const auto existing_content = find_path(nand, content_dir)) {
      require(existing_content->IsDir(),
              "The requested title content path is not a directory");
      for (const auto& child : nand.Children(existing_content->index))
        require(replaced_files.contains(content_dir + "/" + child.name),
                "The requested title has existing content files");
    }
    require_directory(nand, ticket_dir);
    require_directory(nand, "/ticket");
    require_directory(nand, "/title");
    require_directory(nand, "/shared1");
    require_directory(nand, "/sys");
    require_directory(nand, "/title/" + high);
    require_directory(nand, title);
    require_directory(nand, title + "/data");
    require_directory(nand, content_dir);

    const auto uid_data = read_authenticated_file(nand, find_path(nand, "/sys/uid.sys"), 1024 * 1024);
    require(!uid_data.isEmpty(), "NAND UID map is empty");
    UIDmap uid(uid_data);
    require(uid.Check(), "Existing NAND UID map is malformed");
    const auto old_owner = uid.GetUid(job.tid, false);
    const auto owner = uid.GetUid(job.tid);
    require(owner != 0 && uid.Check(), "Cannot assign a valid title UID");

    const auto map_path = QString("/shared1/content.map");
    const auto existing_map = find_path(nand, map_path);
    const auto map_data = existing_map ? read_authenticated_file(nand, existing_map, 1024 * 1024) : QByteArray();
    SharedContentMap shared(map_data);
    require(map_data.isEmpty() || shared.Check(), "Existing shared-content map is malformed");
    SharedContentMap planned_shared(map_data);

    int needed_entries = 0;
    quint64 needed_clusters = 0;
    add_new_entry(nand, ticket_dir, needed_entries);
    add_new_entry(nand, "/title/" + high, needed_entries);
    add_new_entry(nand, title, needed_entries);
    add_new_entry(nand, title + "/data", needed_entries);
    add_new_entry(nand, content_dir, needed_entries);
    add_new_file(nand, ticket_path, ticket.Data().size(), needed_clusters, needed_entries, replaced_files);
    add_new_file(nand, tmd_path, tmd.Data().size(), needed_clusters, needed_entries, replaced_files);

    bool adds_shared = false;
    for (int i = 0; i < tmd.Count(); ++i) {
      const auto size = tmd.Size(i);
      require(size > 0 && quint64(job.data[i + 2].size()) == size,
              "Empty or inconsistent title content is unsupported");
      if (tmd.Type(i) == 0x8001) {
        const auto hash = tmd.Hash(i);
        const auto existing_cid = shared.GetAppFromHash(hash);
        if (!existing_cid.isEmpty()) {
          const auto existing_bytes = read_authenticated_file(nand, find_path(nand, shared_path(existing_cid)), size);
          require(existing_bytes == job.data[i + 2],
                  "Shared-content map references invalid NAND content");
          continue;
        }
        if (!planned_shared.GetAppFromHash(hash).isEmpty()) continue;
        const auto cid = planned_shared.GetNextEmptyCid();
        const auto path = shared_path(cid);
        require(!path_exists(nand, path), "An unindexed shared-content path already exists");
        planned_shared.AddEntry(cid, hash);
        adds_shared = true;
        add_new_file(nand, path, size, needed_clusters, needed_entries, replaced_files);
      } else {
        const auto path = content_dir + "/" + tmd.Cid(i) + ".app";
        add_new_file(nand, path, size, needed_clusters, needed_entries, replaced_files);
      }
    }

    if (owner != old_owner) {
      add_resize(nand, "/sys/uid.sys", uid.Data().size(), needed_clusters, needed_entries);
    }
    if (adds_shared) {
      add_resize(nand, map_path, planned_shared.Data().size(), needed_clusters, needed_entries);
    }

    quint64 released_clusters = 0;
    int released_entries = 0;
    for (const auto& path : replaced_files) {
      const auto entry = find_path(nand, path);
      if (!entry) continue;
      require(entry->IsFile(), "A cIOS replacement path is not a file");
      released_clusters += existing_file_clusters(nand, path);
      ++released_entries;
    }
    working_image::require_capacity(nand, orphan_entries, needed_entries, needed_clusters,
                                    released_entries, released_clusters);
    if (!replaced_files.isEmpty()) {
      for (const auto& path : replaced_files) {
        if (find_path(nand, path))
          require(nand.Delete(path), "Cannot remove the original stock stub file");
      }
    }

    // Existing UID/shared records are persisted in the same operation so the
    // next invocation can reopen the image and assign deterministic IDs.
    require(install_title(nand, uid, shared, validated), "WiiQt failed while writing title files");
    if (owner != old_owner)
      require(nand.SetData("/sys/uid.sys", uid.Data()), "Cannot persist NAND UID map");
    if (adds_shared) {
      const auto entry = find_path(nand, map_path);
      quint16 index = 0;
      if (!entry) index = nand.CreateEntry(map_path, 0, 0, NAND_FILE, NAND_RW, NAND_RW, 0);
      else index = entry->index;
      require(index && nand.SetData(index, shared.Data()), "Cannot persist shared-content map");
    }
    require(nand.WriteMetaData(), "Cannot persist NAND metadata");
  }

  // `replaceable` names the Nintendo title version the slot may hold and lose.
  // The Nintendo stub a cIOS may replace, if its options allow it.
  auto replaceable_stub(const wiiform::CiosOptions& options) -> std::optional<quint16> {
    const bool stub_slot = options.slot == 249 || options.slot == 250;
    require(!options.replace_stock_stub || stub_slot,
            "Stock stub replacement is supported only in slots 249 and 250");
    return options.replace_stock_stub ? std::optional<quint16>(65280) : std::nullopt;
  }

  auto install_validated_wad(std::span<std::uint8_t> image, const ValidatedTitleJob& job,
                             std::optional<quint16> replaceable) -> wiiform::InstalledWad {
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
      auto& nand = working.nand;
      require(path_exists(nand, "/"), "Cannot inspect NAND filesystem tree");
      const auto content = find_path(nand, content_dir);
      const bool occupied = path_exists(nand, ticket_path) ||
          (content && !nand.Children(content->index).isEmpty());
      if (occupied) {
        if (installed_wad_matches(nand, content_dir, ticket_path, job))
          return {title_id, region, version, true};
        require(replaceable.has_value(), "Installed title conflicts with WAD title metadata");
        verify_nintendo_title(nand, title_id, *replaceable);
        const auto current_ticket = find_path(nand, ticket_path);
        require(current_ticket && current_ticket->IsFile(),
                "The occupied slot has no valid ticket file");
        replaced_files.insert(ticket_path);
        const auto current_content = find_path(nand, content_dir);
        require(current_content && current_content->IsDir(),
                "The occupied slot has no content directory");
        for (const auto& child : nand.Children(current_content->index)) {
          require(child.IsFile(), "The occupied slot has unexpected content entries");
          replaced_files.insert(content_dir + "/" + child.name);
        }
        const auto uid_bytes = read_authenticated_file(nand, "/sys/uid.sys", 1024 * 1024);
        UIDmap uid(uid_bytes);
        require(uid.Check() && uid.GetUid(title_id, false) != 0,
                "The replaced Nintendo title has no matching UID record");
      }
      plan_and_install(nand, working.layout.orphan_entries, job, replaced_files);
    }

    working_image::Image readback(image);
    require(installed_wad_matches(readback.nand, content_dir, ticket_path, job),
            "Installed title conflicts with WAD title metadata");
    return {title_id, region, version, false};
  }

} // namespace

namespace title_install {
  auto install_nus_title(std::span<std::uint8_t> image, const QString& cache, quint64 title_id,
                         quint16 version, bool offline) -> wiiform::InstalledWad {
    // NUS serves the TMD and ticket with their certificate chains; the NAND keeps
    // them without, so compare and install the trimmed parts.
    auto downloaded = factory::read_title(cache, title_id, version, offline).get();
    downloaded.data[0] = Tmd(downloaded.data[0]).Data();
    downloaded.data[1] = Ticket(downloaded.data[1], false).Data();
    const auto job = validate_title(std::move(downloaded), "Invalid title contents: " + title_name(title_id));

    const auto region = qFromBigEndian<quint16>(job.get().data[0].constData() + 0x19c);
    const auto tid = title_name(title_id);
    const auto content_dir = "/title/" + tid.left(8) + "/" + tid.mid(8) + "/content";
    const auto ticket_path = "/ticket/" + tid.left(8) + "/" + tid.mid(8) + ".tik";
    working_image::Image working(image);
    auto& nand = working.nand;
    const auto content = find_path(nand, content_dir);
    if (path_exists(nand, ticket_path) || (content && !nand.Children(content->index).isEmpty())) {
      require(installed_wad_matches(nand, content_dir, ticket_path, job),
              "Another version of title " + tid + " is installed");
      return {title_id, region, version, true};
    }
    // The stock build installs every title this way; it verifies the whole image afterwards.
    plan_and_install(nand, working.layout.orphan_entries, job);
    return {title_id, region, version, false};
  }

  auto install_wad(std::span<std::uint8_t> image, const ValidatedTitleJob& job) -> wiiform::InstalledWad {
    return install_validated_wad(image, job, std::nullopt);
  }

  auto install_cios(std::span<std::uint8_t> image, std::span<const std::uint8_t> package,
                    const wiiform::CiosOptions& options) -> wiiform::InstalledWad {
    const auto job = wad_package::read_cios(package, options);
    const auto& parsed = job.get();
    const auto expected_id = (quint64(1) << 32) | options.slot;
    require(parsed.tid == expected_id && parsed.version == options.revision,
            "cIOS WAD does not match its declared slot and revision");
    return install_validated_wad(image, job, replaceable_stub(options));
  }

  auto install_built_cios(std::span<std::uint8_t> image, const QString& cache, bool offline,
                          const wiiform::CiosBuild& build) -> wiiform::InstalledWad {
    const auto base = factory::read_title(cache, (quint64(1) << 32) | build.options.base, build.base_version, offline);
    return install_validated_wad(image, cios_build::derive(base, build), replaceable_stub(build.options));
  }

  auto install_rvloader_ios(std::span<std::uint8_t> image, const QString& cache, bool offline,
                            const wiiform::RvloaderIos& ios) -> wiiform::InstalledWad {
    const auto stock = factory::read_title(cache, ios.source.id, ios.source.version, offline);
    const auto derived = rvloader::derive_ios(stock, ios);
    // In its own slot the derived IOS replaces the stock version it came from;
    // any other slot must be free.
    const bool in_place = (quint64(1) << 32 | ios.slot) == ios.source.id;
    return install_validated_wad(image, derived,
                                 in_place ? std::optional<quint16>(ios.source.version) : std::nullopt);
  }

} // namespace title_install
