// SPDX-License-Identifier: GPL-2.0-only
#include <PriiloaderHacks.hpp>

#include <QCryptographicHash>
#include <QFile>
#include <QFileInfo>
#include <QMap>
#include <QRegularExpression>
#include <stdexcept>

namespace {
  constexpr auto definitions_sha256 = "aea1a0009a327298f0608809c240aaf0ca5f5de3b12ef7ad761901dc4cb74ff4";
  constexpr qsizetype definitions_source_size = 30123;
  constexpr qsizetype definitions_limit = 64 * 1024;

  struct Definition {
    QString name;
    QString master;
    QString required;
    int minimum = 0;
    int maximum = 0;
    bool selector = false;
    bool patch = false;
  };

  void require(bool ok, const char* message) {
    if (!ok) throw std::runtime_error(message);
  }

  auto digest(const QByteArray& bytes) -> QByteArray {
    return QCryptographicHash::hash(bytes, QCryptographicHash::Sha256).toHex();
  }

  auto read_definitions(const QString& path) -> QByteArray {
    const QFileInfo info(path);
    QFile file(path);
    require(info.isFile() && !info.isSymLink() && file.open(QIODevice::ReadOnly) &&
                file.size() > 0 && file.size() <= definitions_limit,
            "Expected a regular Priiloader 0.10.0 hacks_hash.ini file");
    const auto bytes = file.read(definitions_limit + 1);
    require(bytes.size() == file.size() && bytes.size() == definitions_source_size &&
                digest(bytes) == definitions_sha256,
            "Priiloader hacks_hash.ini does not match pinned release 0.10.0");
    auto aligned = bytes;
    aligned.append(QByteArray((32 - aligned.size() % 32) % 32, '\0'));
    return aligned;
  }

  auto verified_padded_definitions(const QByteArray& bytes) -> bool {
    if (bytes.size() != ((definitions_source_size + 31) & ~31)) return false;
    for (qsizetype index = definitions_source_size; index < bytes.size(); ++index)
      if (bytes.at(index) != '\0') return false;
    return digest(bytes.left(definitions_source_size)) == definitions_sha256;
  }

  auto parse_definitions(const QByteArray& bytes, int menu_version) -> QList<Definition> {
    QList<Definition> result;
    QMap<QString, QList<Definition>> masters;
    Definition current;
    auto finish = [&] {
      if (current.name.isEmpty() || current.minimum <= 0 || current.maximum <= 0 ||
          !current.selector || !current.patch)
        return;
      if (!current.required.isEmpty()) {
        bool available = false;
        for (const auto& master : masters.value(current.required))
          available |= master.minimum <= menu_version && menu_version <= master.maximum;
        if (!available) return;
      }
      result.append(current);
      if (!current.master.isEmpty()) masters[current.master].append(current);
    };

    const auto lines = bytes.left(definitions_source_size).split('\n');
    const QRegularExpression header(R"(^\[([^\]]+)\]$)");
    const QRegularExpression positive_integer(R"(^[0-9]+$)");
    for (QByteArray raw_line : lines) {
      auto line = raw_line.trimmed();
      const auto comment = line.indexOf('#');
      if (comment >= 0) line.truncate(comment);
      line = line.trimmed();
      const auto match = header.match(QString::fromUtf8(line));
      if (match.hasMatch()) {
        finish();
        current = Definition{};
        current.name = match.captured(1);
        continue;
      }
      const auto separator = line.indexOf('=');
      if (separator <= 0 || separator == line.size() - 1) continue;
      const auto key = line.left(separator).trimmed();
      const auto value = line.mid(separator + 1).trimmed();
      if (key == "minversion" || key == "maxversion") {
        const auto text = QString::fromLatin1(value);
        bool ok = false;
        const auto number = text.toInt(&ok, 10);
        if (!ok || !positive_integer.match(text).hasMatch()) continue;
        if (key == "minversion") current.minimum = number;
        else current.maximum = number;
      } else if (key == "master") {
        current.master = QString::fromUtf8(value);
      } else if (key == "require") {
        current.required = QString::fromUtf8(value);
      } else if (key == "hash" || key == "offset") {
        if (current.selector && current.patch) current.patch = false;
        current.selector = true;
      } else if (key == "patch") {
        current.patch = true;
      }
    }
    finish();
    return result;
  }

  auto enabled_hacks(const QList<Definition>& definitions, const QByteArray& states,
                     int menu_version) -> QJsonObject {
    require(states.size() == definitions.size(), "Priiloader hack state size differs from pinned definitions");
    QJsonObject result;
    for (int index = 0; index < definitions.size(); ++index) {
      const auto& definition = definitions.at(index);
      if (definition.master.isEmpty() && definition.minimum <= menu_version && menu_version <= definition.maximum)
        result[definition.name] = result[definition.name].toBool() || states.at(index) == 1;
    }
    return result;
  }
}

namespace priiloader {
  auto legacy_fix_480p_definitions() -> QByteArray {
    QByteArray data(
        "[480p graphics fix in system menu]\n"
        "maxversion=518\n"
        "minversion=96\n"
        "amount=2\n"
        "hash=0x38000065,0x9b810019,0x38810018,0x386000e0,0x98010018,0x38a00002\n"
        "patch=0x38000065,0x3c60812f,0x6063efe0,0x7c6803a6,0x4e800021\n"
        "offset=0x812fefe0\n"
        "patch=0x38600003,0x98610019,0x38810018,0x386000e0,0x98010018,0x4e800020\n");
    data.append(QByteArray((32 - data.size() % 32) % 32, '\0'));
    return data;
  }

  auto prepare_hacks(const QString& definitions_path, int menu_version,
                     const QMap<QString, bool>& overrides,
                     const QByteArray& previous_states) -> HackFiles {
    const auto definitions_bytes = read_definitions(definitions_path);
    const auto definitions = parse_definitions(definitions_bytes, menu_version);
    require(!definitions.isEmpty(), "Pinned Priiloader definitions contain no valid hacks");
    auto states = QByteArray(definitions.size(), char(0));
    if (!previous_states.isEmpty()) {
      require(previous_states.size() == states.size(), "Existing Priiloader hack state has the wrong size");
      for (const char state : previous_states)
        require(state == 0 || state == 1, "Invalid Priiloader hack state");
      states = previous_states;
    }
    HackFiles result{definitions_bytes, states, {}};
    for (auto it = overrides.cbegin(); it != overrides.cend(); ++it) {
      bool found = false;
      for (int index = 0; index < definitions.size(); ++index) {
        const auto& definition = definitions.at(index);
        if (definition.name != it.key() || !definition.master.isEmpty() ||
            definition.minimum > menu_version || menu_version > definition.maximum)
          continue;
        result.states[index] = it.value() ? char(1) : char(0);
        found = true;
      }
      require(found, "Requested Priiloader hack title is unavailable for this System Menu version");
      result.selected[it.key()] = it.value();
    }
    return result;
  }

  auto inspect_hacks(const QByteArray& definitions, const QByteArray& states,
                     int menu_version) -> QJsonObject {
    if (definitions == priiloader::legacy_fix_480p_definitions()) {
      require(states.size() == 1 && (states[0] == 0 || states[0] == 1), "Invalid legacy Priiloader hack state");
      return {{"fix_480p", states.at(0) == 1}};
    }
    require(verified_padded_definitions(definitions),
            "Priiloader hack definitions do not match pinned release 0.10.0");
    const auto parsed = parse_definitions(definitions, menu_version);
    return enabled_hacks(parsed, states, menu_version);
  }
}
