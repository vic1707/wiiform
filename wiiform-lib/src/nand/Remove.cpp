// SPDX-License-Identifier: GPL-2.0-only
#include <Remove.hpp>

#include <Titles.hpp>
#include <WorkingImage.hpp>
#include <tiktmd.h>

#include <QSet>
#include <QtEndian>
#include <optional>
#include <stdexcept>

namespace {
  using working_image::find_path;
  using working_image::read_authenticated_file;

  void require(bool ok, const char* message) {
    if (!ok) throw std::runtime_error(message);
  }

  auto title_id_is_removable(quint64 title_id) -> bool {
    switch (title_id) {
      case 0x0001000248414141ull: // HAAA: Photo Channel base, v1.0 or the v1.1-installed stub.
      case 0x0001000248414241ull: // HABA: Wii Shop Channel.
      case 0x0001000248415941ull: // HAYA: Photo Channel 1.1.
      case 0x0001000248414641ull: // HAFA: Forecast common stub.
      case 0x0001000248414645ull: // HAFE: Forecast USA.
      case 0x000100024841464aull: // HAFJ: Forecast Japan.
      case 0x0001000248414650ull: // HAFP: Forecast Europe.
      case 0x0001000248414741ull: // HAGA: News common stub.
      case 0x0001000248414745ull: // HAGE: News USA.
      case 0x000100024841474aull: // HAGJ: News Japan.
      case 0x0001000248414750ull: // HAGP: News Europe.
        return true;
      default:
        return false;
    }
  }

  auto title_name(quint64 title_id) -> QString {
    return QString("%1").arg(title_id, 16, 16, QChar('0'));
  }

  void remove_title_contents(NandBin& nand, quint64 id) {
    const auto name = title_name(id);
    const auto high = name.left(8);
    const auto low = name.mid(8);
    const auto title_path = "/title/" + high + "/" + low;
    const auto content_path = title_path + "/content";
    const auto data_path = title_path + "/data";
    const auto ticket_path = "/ticket/" + high + "/" + low + ".tik";
    const auto data = find_path(nand, data_path);
    const auto content = find_path(nand, content_path);
    const auto ticket_entry = find_path(nand, ticket_path);
    require(data && content && ticket_entry, "Allowlisted channel is not completely installed");
    require(data->IsDir() && content->IsDir() && ticket_entry->IsFile(),
            "Installed channel has malformed title paths");

    auto read_file = [&](const QString& path, quint64 limit) {
      return read_authenticated_file(nand, path, limit);
    };
    const auto title_tmd = read_file(content_path + "/title.tmd", 2 * 1024 * 1024);
    const auto ticket = read_file(ticket_path, 64 * 1024);
    require(CheckTitleMetadata(title_tmd, ticket), "Installed channel TMD or ticket is malformed");
    const auto count = qFromBigEndian<quint16>(title_tmd.constData() + 0x1de);
    require(title_tmd.size() == 0x1e4 + count * 0x24 && ticket.size() == 0x2a4,
            "Installed channel TMD or ticket has an unsupported size");
    Tmd tmd(title_tmd);
    Ticket tik(ticket, false);
    require(tmd.Tid() == id && tik.Tid() == id && tmd.Count() > 0,
            "Installed channel metadata does not match its title ID");
    const auto app_count = tmd.Count();
    quint64 total_size = 0;
    QSet<QString> expected_paths{"title.tmd"};
    QSet<quint16> indices;
    QSet<QString> content_ids;
    std::optional<SharedContentMap> shared;
    for (int i = 0; i < app_count; ++i) {
      const auto size = tmd.Size(i);
      require(size > 0 && size <= 0x20000000 - total_size &&
                  (tmd.Type(i) == 1 || tmd.Type(i) == 0x8001),
              "Installed channel content table is unsupported");
      total_size += size;
      require(!indices.contains(tmd.Index(i)), "Installed channel TMD has duplicate content indices");
      indices.insert(tmd.Index(i));
      const auto cid = tmd.Cid(i);
      require(!content_ids.contains(cid), "Installed channel TMD has duplicate content IDs");
      content_ids.insert(cid);
      QString app_path;
      if (tmd.Type(i) == 1) {
        expected_paths.insert(cid + ".app");
        app_path = content_path + "/" + cid + ".app";
      } else {
        if (!shared) {
          shared.emplace(read_file("/shared1/content.map", 1024 * 1024));
          require(shared->Check(), "NAND shared-content map is malformed");
        }
        const auto name = shared->GetAppFromHash(tmd.Hash(i));
        require(!name.isEmpty(), "Installed channel shared content is missing from its map");
        app_path = "/shared1/" + name + ".app";
      }
      const auto app = read_file(app_path, size);
      require(quint64(app.size()) == size && GetSha1(app) == tmd.Hash(i),
              "Installed channel content does not match its TMD");
    }
    const auto content_files = nand.Children(content->index);
    for (const auto& child : content_files)
      require(child.IsFile() && expected_paths.contains(child.name),
              "Installed channel content directory has unexpected files");
    require(content_files.size() == expected_paths.size(),
            "Installed channel is missing a TMD or private content file");
    const auto uid_bytes = read_file("/sys/uid.sys", 1024 * 1024);
    UIDmap uid(uid_bytes);
    require(uid.Check() && uid.GetUid(id, false) != 0,
            "Installed channel has no matching UID record");

    require(nand.Delete(content_path) && nand.Delete(ticket_path),
            "Cannot remove the installed channel content and ticket");
    require(nand.WriteMetaData(), "Cannot persist channel removal metadata");
  }
}

namespace title_remove {
  void remove_channel(std::span<std::uint8_t> image, quint64 title_id) {
    require(title_id_is_removable(title_id),
            "Only allowlisted Forecast, News, Shop and Photo Channel titles can be removed");
    working_image::Image working(image);
    remove_title_contents(working.nand, title_id);
  }

}
