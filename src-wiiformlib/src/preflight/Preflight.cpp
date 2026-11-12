// SPDX-License-Identifier: GPL-2.0-only
#include <Preflight.hpp>
#include <BootDiagnostics.hpp>
#include <Inspection.hpp>
#include <Layout.hpp>
#include <Seed.hpp>

#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <algorithm>
#include <functional>
#include <memory>
#include <stdexcept>

#ifndef WIICLI_VERSION
#define WIICLI_VERSION "unknown"
#endif
#ifndef WIICLI_REVISION
#define WIICLI_REVISION "unknown"
#endif

namespace {
  constexpr qint64 nand_size = 0x21000000;
  constexpr int boot_size = 0x108000;

  void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }

  // Legacy library diagnostics are not a private evidence format. Only our
  // explicit, payload-free checks are emitted by this command.
  struct QuietQt {
    QtMessageHandler previous = qInstallMessageHandler([](QtMsgType, const QMessageLogContext&, const QString&) {});
    ~QuietQt() { qInstallMessageHandler(previous); }
  };

  auto digest(QFile& file) -> QString {
    const auto position = file.pos();
    require(file.seek(0), "Cannot seek required input");
    QCryptographicHash hash(QCryptographicHash::Sha256);
    require(hash.addData(&file) && file.error() == QFileDevice::NoError, "Cannot hash required input");
    require(file.seek(position), "Cannot seek required input");
    return QString::fromLatin1(hash.result().toHex());
  }

  auto digest(const QString& path) -> QString {
    QFile file(path);
    require(QFileInfo(path).isFile() && file.open(QIODevice::ReadOnly), "Cannot read required input");
    return digest(file);
  }

  auto json_blocks(const QList<int>& blocks) -> QJsonArray {
    QJsonArray result;
    for (const auto block : blocks) result.append(block);
    return result;
  }

  auto bad_blocks(NandBin& nand) -> QList<int> {
    const auto fats = nand.GetFats();
    require(fats.size() == 0x8000, "Invalid filesystem allocation table");
    QList<int> result;
    for (int block = 8; block < 4096; ++block) {
      int bad = 0;
      for (int cluster = 0; cluster < 8; ++cluster) bad += fats[block * 8 + cluster] == 0xfffd;
      require(!bad || bad == 8, "Inconsistent filesystem bad-block records");
      if (bad) result.append(block);
    }
    return result;
  }

  struct Image {
    QString path;
    QByteArray keys, boots;
    std::unique_ptr<NandBin> nand;
    quint16 active_metadata = 0;
    bool loaded = false, identity = false, layout = false, authenticated = false;
  };
}

namespace preflight {
  auto inspect(const QString& seed_path, const QString& reference_path, const QString& candidate_path,
               const nand_build::Options& options) -> QJsonObject {
    QuietQt quiet;
    QJsonArray checks;
    QJsonObject hashes, key_sources, recorded_blocks, layout_summary, filesystem_summary, stock_summary, metadata_summary;
    QMap<QString, QString> observed;
    bool failed = false, blocked = false;
    auto remember = [&](const QString& path, const QString& hash) {
      require(!observed.contains(path) || observed.value(path) == hash, "An input changed while checks were running");
      observed[path] = hash;
    };
    auto add = [&](const QString& id, const QString& status, const QString& detail, bool host = true) {
      checks.append(QJsonObject{{"id", id}, {"status", status}, {"detail", detail}});
      if (host) {
        failed |= status == "fail";
        blocked |= status == "blocked";
      }
    };
    auto attempt = [&](const QString& id, bool ready, const std::function<void()>& operation) {
      if (!ready) {
        add(id, "blocked", "Required input or prerequisite check did not pass");
        return false;
      }
      try {
        operation();
        add(id, "pass", "Check completed");
        return true;
      } catch (const YAML::Exception&) {
        add(id, "fail", "Invalid seed field or YAML value");
      } catch (const std::exception& error) {
        // All operations here throw fixed validation messages. YAML decoding
        // additionally sanitizes parser/conversion exceptions in seed::read.
        add(id, "fail", QString::fromUtf8(error.what()));
      } catch (...) {
        add(id, "fail", "Input could not be validated");
      }
      return false;
    };

    seed::Components seed;
    const auto seed_ok = attempt("seed", true, [&] {
      const auto hash = digest(seed_path);
      remember(seed_path, hash);
      hashes["seed"] = hash;
      seed = ::seed::read(seed_path);
    });
    nand_build::Profile profile;
    const auto profile_ok = attempt("profile", seed_ok, [&] {
      profile = nand_build::resolve_profile(seed, options);
    });
    QList<int> seed_bad;
    if (seed_ok) {
      for (const auto& entry : seed.manifest["bad_blocks"]) seed_bad.append(entry.as<int>());
      std::sort(seed_bad.begin(), seed_bad.end());
      recorded_blocks["seed"] = json_blocks(seed_bad);
    }

    auto load = [&](Image& image, const QString& label, const QString& path, bool generated) {
      image.path = path;
      image.loaded = attempt(label + ".input", true, [&] {
        QFile file(path);
        require(QFileInfo(path).isFile() && file.open(QIODevice::ReadOnly), "Cannot read NAND input");
        const auto size = file.size();
        require(size == nand_size || size == nand_size + 0x400, "Expected a full NAND dump with spare data");
        const auto hash = digest(file);
        remember(path, hash);
        hashes[label] = hash;
        image.boots = file.read(boot_size);
        require(image.boots.size() == boot_size, "Cannot read NAND boot area");
        if (size == nand_size + 0x400) {
          require(file.seek(nand_size), "Cannot read NAND key footer");
          image.keys = file.read(0x400);
          require(image.keys.size() == 0x400, "Truncated NAND key footer");
          key_sources[label] = "footer";
        }
        const auto sidecar_path = QFileInfo(path).absoluteDir().filePath("keys.bin");
        const QFileInfo sidecar_info(sidecar_path);
        if (sidecar_info.exists() || sidecar_info.isSymLink()) {
          QFile sidecar(sidecar_path);
          require(sidecar_info.isFile() && sidecar.open(QIODevice::ReadOnly) && sidecar.size() == 0x400,
                  "Invalid sidecar keys file");
          remember(sidecar_path, digest(sidecar));
          const auto keys = sidecar.read(0x401);
          require(keys.size() == 0x400 && sidecar.error() == QFileDevice::NoError, "Cannot read sidecar keys");
          require(image.keys.isEmpty() || image.keys == keys, "Footer and sidecar keys disagree");
          if (image.keys.isEmpty()) {
            image.keys = keys;
            key_sources[label] = "sidecar";
          } else key_sources[label] = "footer-and-sidecar";
        }
        require(image.keys.size() == 0x400, "NAND input needs matching footer or sidecar keys");
      });
      image.identity = attempt(label + ".identity", seed_ok && image.loaded, [&] {
        require(image.keys == seed.files.value("keys.bin"), "NAND keys do not match the console seed");
      });
      image.layout = attempt(label + ".layout", image.identity, [&] {
        const auto summary = ::inspect::validate_layout(path, generated);
        image.active_metadata = summary.active_metadata;
        layout_summary[label] = QJsonObject{{"active_metadata", summary.active_metadata},
                                            {"unowned_clusters", summary.unowned_clusters},
                                            {"reserved_clusters", summary.reserved_clusters},
                                            {"orphan_entries", summary.orphan_entries}};
      });
      if (image.layout) {
        const auto summary = layout_summary[label].toObject();
        const auto review = summary["unowned_clusters"].toInt() || summary["reserved_clusters"].toInt() ||
                            summary["orphan_entries"].toInt();
        add(label + ".allocation", review ? "warn" : "pass",
            review ? "Historical non-live allocation records retained; live chains and files are checked separately" :
                     "No unowned or unexpected reserved data allocations");
      } else add(label + ".allocation", "blocked", "Layout validation did not pass");
      image.authenticated = attempt(label + ".authentication", image.layout, [&] {
        image.nand = std::make_unique<NandBin>();
        require(image.nand->SetPath(path, NandBin::Access::ReadOnly) && image.nand->InitNand(), "Cannot read NAND filesystem");
        require(image.nand->Keys() == image.keys, "Filesystem decoder selected different keys");
        require(image.nand->CheckHmacMeta(image.active_metadata), "Selected filesystem metadata does not authenticate");
        bool valid = false;
        for (int cluster = 0x7f00; cluster < 0x8000 && !valid; cluster += 16)
          valid = image.nand->CheckHmacMeta(cluster);
        require(valid, "No filesystem superblock authenticates with the console keys");
      });
      attempt(label + ".settings", image.authenticated && (generated ? profile_ok : seed_ok), [&] {
        require(seed.files.contains("setting.txt"), "Seed has no console settings");
        const auto expected = generated ? profile.settings : seed.files.value("setting.txt");
        require(::inspect::read_file(*image.nand, "/title/00000001/00000002/data/setting.txt", 4096) ==
                    expected, "NAND settings do not match the expected console settings");
      });
    };

    Image reference, candidate;
    const bool reference_requested = !reference_path.isEmpty();
    if (reference_requested) {
      load(reference, "reference", reference_path, false);
      if (seed_ok && reference.loaded) {
        if (reference.boots == seed.files.value("boot-blocks.bin"))
          add("reference.boot", "pass", "Reference boot bytes match the seed");
        else add("reference.boot", "blocked", "Live/reference boot differs from the seed; recovery changes require explicit review");
      } else add("reference.boot", "blocked", "Required input or prerequisite check did not pass");
    }
    load(candidate, "candidate", candidate_path, true);
    attempt("candidate.boot", seed_ok && candidate.loaded, [&] {
      require(candidate.boots == seed.files.value("boot-blocks.bin"), "Candidate boot bytes differ from the console seed");
    });
    const bool stock_boot = options.boot_policy == ::inspect::BootPolicy::Stock;
    const auto boot_ok = attempt(stock_boot ? "candidate.boot_signature" : "candidate.boot_structure",
                                 candidate.loaded, [&] { ::inspect::verify_boot(candidate.boots, options.boot_policy); });
    if (!stock_boot && boot_ok)
      add("candidate.boot_signature", "info", "Source boot structure checked. Signature authenticity is unverified");

    auto check_boot_ecc = [&](const Image& image, const QString& label) -> QJsonArray {
      QJsonArray errors;
      if (!image.loaded) {
        add(label + ".boot_ecc", "blocked", "NAND input could not be read");
        return errors;
      }
      const auto diagnostics = diagnose_boot_ecc(image.boots,
          seed_ok ? seed.files.value("boot-blocks.bin") : QByteArray(), options.boot_policy);
      add(label + ".boot_ecc", diagnostics.pages.isEmpty() ? "pass" : diagnostics.blocked ? "blocked" : "info",
          diagnostics.pages.isEmpty() ? "No non-erased boot-page ECC mismatches" : diagnostics.blocked ?
              "Boot-page ECC mismatches need per-image classification; no page-number exemption applied" :
              "Validated redundant boot2-map pages have unwritten ECC; boot bytes are unchanged");
      return diagnostics.pages;
    };
    const auto boot_ecc = check_boot_ecc(candidate, "candidate");
    const auto reference_boot_ecc = reference_requested ? check_boot_ecc(reference, "reference") : QJsonArray();

    auto compare_bad = [&](Image& image, const QString& label, bool generated) {
      QList<int> actual;
      const auto read_ok = attempt(label + ".bad_block_records", image.authenticated,
                                   [&] { actual = bad_blocks(*image.nand); });
      if (!read_ok || !seed_ok) {
        add(label + ".bad_blocks", "blocked", "Required input or prerequisite check did not pass");
        return;
      }
      recorded_blocks[label] = json_blocks(actual);
      if (actual == seed_bad) add(label + ".bad_blocks", "pass", "Recorded filesystem bad blocks match the seed");
      else add(label + ".bad_blocks", generated ? "fail" : "blocked",
               "Recorded bad blocks changed; this comparison is not a physical NAND health scan");
    };
    if (reference_requested) compare_bad(reference, "reference", false);
    compare_bad(candidate, "candidate", true);
    auto filesystem = [&](Image& image, const QString& label, bool generated) {
      attempt(label + ".filesystem", image.authenticated, [&] {
        const auto summary = ::inspect::verify_filesystem(*image.nand, generated, image.active_metadata);
        filesystem_summary[label] = QJsonObject{{"checked_clusters", summary.checked_clusters},
                                                {"checked_files", summary.checked_files},
                                                {"authenticated_metadata_copies", summary.authenticated_metadata_copies}};
      });
    };
    if (reference_requested) filesystem(reference, "reference", false);
    filesystem(candidate, "candidate", true);
    attempt("candidate.stock", candidate.authenticated && profile_ok, [&] {
      const auto summary = ::inspect::verify_stock(*candidate.nand, profile.area);
      stock_summary = QJsonObject{{"checked_titles", summary.checked_titles}, {"checked_contents", summary.checked_contents}};
    });
    attempt("candidate.metadata", candidate.authenticated && profile_ok, [&] {
      const auto summary = ::inspect::verify_metadata(*candidate.nand, profile.area);
      metadata_summary = QJsonObject{{"checked_entries", summary.checked_entries}, {"checked_titles", summary.checked_titles}};
    });

    attempt("inputs.unchanged", !observed.isEmpty(), [&] {
      for (auto input = observed.cbegin(); input != observed.cend(); ++input)
        require(digest(input.key()) == input.value(), "An input changed while checks were running");
    });
    add("recovery", "blocked", "BootMii-as-boot2 cold-boot recovery must be qualified on the named console", false);
    add("restore_boundary", "blocked", "Exact restore and rollback write behavior needs separate evidence", false);
    add("hardware", "blocked", "Physical restore, first boot, features and rollback have not been tested by this command", false);

    QJsonObject tool{{"version", WIICLI_VERSION}, {"source_revision", WIICLI_REVISION}};
    try { tool["executable_sha256"] = digest(QCoreApplication::applicationFilePath()); }
    catch (...) { tool["executable_sha256"] = QJsonValue(); }
    QJsonObject report{{"schema_version", 1}, {"tool", tool}, {"profile", "stock-4.3-r1"},
                       {"created_utc", QDateTime::currentDateTimeUtc().toString(Qt::ISODate)},
                       {"host_status", failed ? "fail" : blocked ? "blocked" : "pass"},
                       {"restore_status", "blocked"}, {"input_sha256", hashes}, {"key_sources", key_sources},
                       {"recorded_bad_blocks", recorded_blocks}, {"boot_ecc", boot_ecc},
                       {"reference_boot_ecc", reference_boot_ecc},
                       {"layout_summary", layout_summary}, {"filesystem_summary", filesystem_summary},
                       {"stock_summary", stock_summary}, {"metadata_summary", metadata_summary}, {"checks", checks}};
    QJsonObject overrides;
    for (auto field = options.setting_overrides.cbegin(); field != options.setting_overrides.cend(); ++field)
      overrides[field.key()] = field.value();
    report["setting_overrides"] = overrides;
    report["reference_requested"] = reference_requested;
    report["boot_policy"] = nand_build::boot_policy_name(options.boot_policy);
    report["boot_authenticity"] = !boot_ok ? "unverified" :
        stock_boot ? "verified-stock" : "preserved-source-unverified";
    if (candidate.loaded)
      report["boot_area_sha256"] = QString::fromLatin1(QCryptographicHash::hash(candidate.boots, QCryptographicHash::Sha256).toHex());
    if (profile_ok) {
      report["region"] = profile.area;
      report["settings_sha256"] = QString::fromLatin1(QCryptographicHash::hash(profile.settings, QCryptographicHash::Sha256).toHex());
    }
    return report;
  }

}
