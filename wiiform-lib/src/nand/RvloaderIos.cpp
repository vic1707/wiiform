// SPDX-License-Identifier: GPL-2.0-only
// Mirrors what RVLoader v2.1's installer (installer/source/iosinst.cpp, GPL-3.0,
// https://github.com/Aurelio92/RVLoader) does to an IOS whose modules are on the
// NAND. Only the facts are reproduced here: the module layout, the byte patterns
// and the replacement bytes.
#include <RvloaderIos.hpp>
#include <Support.hpp>

#include <tiktmd.h>

#include <QCryptographicHash>
#include <QtEndian>
#include <algorithm>
#include <array>
#include <cstring>

namespace {
  constexpr int tmd_header_size = 0x1e4;
  constexpr int content_record_size = 0x24;
  constexpr quint16 shared_content = 0x8001;

  struct Module {
    QByteArray bytes;
    QByteArray sha1;
  };

  auto sha1_of(const QByteArray& bytes) -> QByteArray {
    return QCryptographicHash::hash(bytes, QCryptographicHash::Sha1);
  }

  // The stock Wi-Fi driver (WD) and Wi-Fi library (WL) modules the installer removes.
  struct WifiModules {
    std::uint32_t ios;
    const char* wd;
    const char* wl;
  };
  constexpr std::array wifi_modules = {
      WifiModules{36, "41f6574294b4b35e6e401435a20cbdc09e36f53f", "d1f60d8643c8dcb472d8415a2e458a64bc5d14dc"},
      WifiModules{58, "79a9f215ea9b38fd085da32720a7d39818c8701d", "d1f60d8643c8dcb472d8415a2e458a64bc5d14dc"},
      WifiModules{80, "41f6574294b4b35e6e401435a20cbdc09e36f53f", "d1f60d8643c8dcb472d8415a2e458a64bc5d14dc"},
  };

  auto bytes(std::initializer_list<unsigned char> values) -> QByteArray {
    QByteArray result;
    for (const auto value : values) result.append(char(value));
    return result;
  }

  // Replaces every non-overlapping occurrence and returns how many there were.
  auto replace_all(QByteArray& kernel, const QByteArray& pattern, const QByteArray& replacement) -> int {
    int count = 0;
    for (auto at = kernel.indexOf(pattern); at >= 0; at = kernel.indexOf(pattern, at + pattern.size())) {
      kernel.replace(at, replacement.size(), replacement);
      ++count;
    }
    return count;
  }

  // The fakesigning ("trucha") bug, ES_Identify, ISFS permissions and the ES
  // downgrade check, patched out of the kernel.
  void patch_kernel(QByteArray& kernel) {
    // The installer patches either spelling of the hash check.
    const auto hash_checks = replace_all(kernel, bytes({0x20, 0x07, 0x4b, 0x0b}), bytes({0x20, 0x00, 0x4b, 0x0b})) +
                             replace_all(kernel, bytes({0x20, 0x07, 0x23, 0xa2}), bytes({0x20, 0x00, 0x23, 0xa2}));
    require(hash_checks > 0, "IOS kernel has no hash check to patch");
    require(replace_all(kernel, bytes({0x28, 0x03, 0xd1, 0x23}), bytes({0x28, 0x03, 0x00, 0x00})) > 0,
            "IOS kernel has no ES_Identify check to patch");
    require(replace_all(kernel, bytes({0x42, 0x8b, 0xd0, 0x01, 0x25, 0x66}),
                        bytes({0x42, 0x8b, 0xe0, 0x01, 0x25, 0x66})) > 0,
            "IOS kernel has no ISFS permission check to patch");
    require(replace_all(kernel, bytes({0xd2, 0x01, 0x4e, 0x56}), bytes({0xe0, 0x01, 0x4e, 0x00})) > 0,
            "IOS kernel has no version check to patch");
  }

  auto content_record(quint32 cid, quint16 index, const Module& module) -> QByteArray {
    QByteArray record(content_record_size, '\0');
    auto* out = reinterpret_cast<uchar*>(record.data());
    qToBigEndian<quint32>(cid, out);
    qToBigEndian<quint16>(index, out + 4);
    qToBigEndian<quint16>(shared_content, out + 6);
    qToBigEndian<quint64>(quint64(module.bytes.size()), out + 8);
    record.replace(16, 20, module.sha1);
    return record;
  }
}

namespace rvloader {
  auto derive_ios(const ValidatedTitleJob& validated, const wiiform::RvloaderIos& ios) -> ValidatedTitleJob {
    const auto& stock = validated.get();
    const auto source = static_cast<quint32>(stock.tid);
    const auto wifi = std::find_if(wifi_modules.begin(), wifi_modules.end(),
                                   [&](const auto& entry) { return entry.ios == source; });
    require(stock.tid >> 32 == 1 && wifi != wifi_modules.end(),
            "RVLoader IOS can only be derived from IOS36, IOS58 or IOS80");
    require(ios.slot >= 3 && ios.slot <= 253, "RVLoader IOS slots are 3 through 253");

    // The installer rebuilds the TMD from the shared modules alone, in TMD order,
    // numbering them from zero; the small local content is dropped.
    Tmd tmd(stock.data[0]);
    QList<Module> modules;
    int boot = -1;
    for (quint16 i = 0; i < tmd.Count(); ++i) {
      if (tmd.Type(i) != shared_content) continue;
      if (tmd.Index(i) == tmd.BootIndex()) boot = int(modules.size());
      modules.append({stock.data[i + 2], tmd.Hash(i)});
    }
    require(boot >= 0, "Stock IOS kernel is not a shared module");

    if (ios.es_patches) {
      patch_kernel(modules[boot].bytes);
      modules[boot].sha1 = sha1_of(modules[boot].bytes);
    }

    // Content IDs are the module positions; the replacement Wi-Fi driver takes the next one.
    QList<quint32> cids;
    for (int i = 0; i < modules.size(); ++i) cids.append(quint32(i));
    if (!ios.wifi_module.empty()) {
      const auto wd = QByteArray::fromHex(wifi->wd), wl = QByteArray::fromHex(wifi->wl);
      int removed = 0;
      for (int i = int(modules.size()) - 1; i >= 0; --i) {
        if (modules[i].sha1 != wd && modules[i].sha1 != wl) continue;
        if (i < boot) --boot;
        modules.removeAt(i);
        cids.removeAt(i);
        ++removed;
      }
      require(removed == 2, "Stock IOS lacks the Wi-Fi modules RVLoader replaces");
      const QByteArray driver(reinterpret_cast<const char*>(ios.wifi_module.data()), int(ios.wifi_module.size()));
      modules.append({driver, sha1_of(driver)});
      cids.append(*std::max_element(cids.begin(), cids.end()) + 1);
    }

    auto metadata = stock.data[0].left(tmd_header_size);
    qToBigEndian<quint16>(quint16(modules.size()), reinterpret_cast<uchar*>(metadata.data() + 0x1de));
    qToBigEndian<quint16>(quint16(boot), reinterpret_cast<uchar*>(metadata.data() + 0x1e0));
    for (int i = 0; i < modules.size(); ++i) metadata += content_record(cids[i], quint16(i), modules[i]);

    const auto title_id = (quint64(1) << 32) | ios.slot;
    // The highest version keeps system updates from replacing the title.
    constexpr quint16 version = 0xffff;
    Tmd derived(metadata);
    require(derived.SetTid(title_id) && derived.SetVersion(version) && derived.FakeSign(),
            "Cannot fakesign the RVLoader IOS metadata");
    metadata = derived.Data();
    // "Makes IOS 58 as recognizable as valid to be loaded from homebrews": the
    // installer marks the IOS58 slot's zeroed signature this way.
    if (ios.slot == 58) metadata.replace(8, 4, QByteArray(4, char(0xff)));

    Ticket ticket(stock.data[1], false);
    require(ticket.SetTid(title_id) && ticket.FakeSign(), "Cannot fakesign the RVLoader IOS ticket");

    NusJob job;
    job.tid = title_id;
    job.version = version;
    job.decrypt = true;
    job.data << metadata << ticket.Data();
    for (const auto& module : modules) job.data << module.bytes;
    return validate_title(std::move(job), "Derived RVLoader IOS is inconsistent");
  }
}
