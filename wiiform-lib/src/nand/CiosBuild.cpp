// SPDX-License-Identifier: GPL-2.0-only
// Applies a d2x ciosmaps.xml base entry (patches and modules) to a stock IOS,
// as d2x's installer and WiiPy's ciosbuild (MIT, https://github.com/NinjaCheetah/WiiPy) do.
#include <CiosBuild.hpp>
#include <Support.hpp>
#include <TitleBuild.hpp>
#include <WadPackage.hpp>

#include <tiktmd.h>

#include <algorithm>

namespace {
  auto bytes(std::span<const std::uint8_t> span) -> QByteArray {
    return QByteArray(reinterpret_cast<const char*>(span.data()), int(span.size()));
  }
}

namespace cios_build {
  auto derive(const ValidatedTitleJob& validated, const wiiform::CiosBuild& build) -> ValidatedTitleJob {
    const auto& stock = validated.get();
    const auto& options = build.options;
    require(stock.tid == ((quint64(1) << 32) | options.base) && stock.version == build.base_version,
            "The base IOS does not match the d2x map");
    require(options.slot >= 248 && options.slot <= 251, "Supported d2x cIOS slots are 248 through 251");

    Tmd tmd(stock.data[0]);
    QList<title_build::Content> contents;
    quint32 boot_cid = 0;
    for (quint16 i = 0; i < tmd.Count(); ++i) {
      const auto cid = tmd.Cid(i).toUInt(nullptr, 16);
      if (tmd.Index(i) == tmd.BootIndex()) boot_cid = cid;
      contents.append({cid, tmd.Type(i), stock.data[i + 2]});
    }
    const auto position_of = [&](quint32 cid) {
      const auto found = std::find_if(contents.begin(), contents.end(), [&](const auto& c) { return c.cid == cid; });
      require(found != contents.end(), "The d2x map names a content the base IOS lacks");
      return int(found - contents.begin());
    };

    // Patched contents become the title's own: no longer shared with the stock IOS.
    for (const auto& patch : build.patches) {
      auto& content = contents[position_of(patch.cid)];
      const auto original = bytes(patch.original);
      require(patch.original.size() == patch.replacement.size() &&
                  quint64(patch.offset) + original.size() <= quint64(content.bytes.size()) &&
                  content.bytes.mid(int(patch.offset), original.size()) == original,
              "The base IOS does not hold the bytes a d2x patch expects");
      content.bytes.replace(int(patch.offset), original.size(), bytes(patch.replacement));
      content.type = title_build::normal_content;
    }

    // A module either goes last, or takes a position and sends its content last.
    for (const auto& module : build.modules) {
      const title_build::Content added{module.cid, title_build::normal_content, bytes(module.bytes)};
      require(std::none_of(contents.begin(), contents.end(), [&](const auto& c) { return c.cid == module.cid; }),
              "A d2x module reuses a content ID of the base IOS");
      if (module.position < 0) {
        contents.append(added);
        continue;
      }
      require(module.position < contents.size(), "A d2x module position is outside the base IOS");
      const auto displaced = contents[module.position];
      contents[module.position] = added;
      contents.append(displaced);
    }

    const auto title_id = (quint64(1) << 32) | options.slot;
    const auto metadata = title_build::retitle_tmd(
        title_build::tmd(stock.data[0], contents, quint16(position_of(boot_cid))), title_id, options.revision);
    auto job = title_build::job(title_id, options.revision, metadata,
                                title_build::retitle_ticket(stock.data[1], title_id), contents,
                                "The built cIOS is inconsistent");
    wad_package::check_cios(job, options);
    return job;
  }
}
