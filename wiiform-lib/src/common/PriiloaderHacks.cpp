// SPDX-License-Identifier: GPL-2.0-only
#include <PriiloaderHacks.hpp>

#include <QCryptographicHash>
#include <QMap>
#include <QRegularExpression>
#include <QSet>
#include <array>
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

  auto validate_definitions(QByteArray bytes) -> QByteArray {
    require(bytes.size() > 0 && bytes.size() <= definitions_limit &&
                bytes.size() == definitions_source_size && digest(bytes) == definitions_sha256,
            "Priiloader hacks_hash.ini does not match pinned release 0.10.0");
    auto aligned = bytes;
    aligned.append(QByteArray((32 - aligned.size() % 32) % 32, '\0'));
    return aligned;
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

}

namespace priiloader {
  auto prepare_hacks(std::span<const std::uint8_t> input, int menu_version,
                     std::span<const wiiform::Hack> enabled_hacks) -> HackFiles {
    require(!input.empty() && input.size() <= definitions_limit,
            "Expected a bounded Priiloader 0.10.0 hacks_hash.ini file");
    auto view = QByteArray::fromRawData(reinterpret_cast<const char*>(input.data()),
                                        static_cast<int>(input.size()));
    const auto definitions_bytes = validate_definitions(view);
    const auto definitions = parse_definitions(definitions_bytes, menu_version);
    require(!definitions.isEmpty(), "Pinned Priiloader definitions contain no valid hacks");
    HackFiles result{definitions_bytes, QByteArray(definitions.size(), '\0')};
    constexpr std::array names{
        "480p graphics fix in system menu", "Block Online Updates", "Block Disc Updates",
        "Block Disc Autoboot", "Auto-Press A at Health Screen", "Remove Deflicker",
        "Move Disc Channel", "Remove NoCopy Save File Protection", "Replace Health Screen with Backmenu",
        "No System Menu Background Music", "Wiimmfi Patch v4", "Region Free EVERYTHING"};
    QSet<QString> requested;
    for (const auto hack : enabled_hacks) {
      const auto option = static_cast<std::size_t>(hack);
      require(option < names.size(), "Unsupported Priiloader hack");
      auto name = QString::fromLatin1(names[option]);
      if (hack == wiiform::Hack::region_free_everything && menu_version == 512) name += " JP";
      require(!requested.contains(name), "Duplicate Priiloader hack");
      requested.insert(name);
      bool found = false;
      for (int index = 0; index < definitions.size(); ++index) {
        const auto& definition = definitions.at(index);
        if (definition.name != name || !definition.master.isEmpty() ||
            definition.minimum > menu_version || menu_version > definition.maximum)
          continue;
        result.states[index] = char(1);
        found = true;
      }
      require(found, "Requested Priiloader hack title is unavailable for this System Menu version");
    }
    return result;
  }
}
