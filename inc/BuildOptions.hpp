// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <Inspection.hpp>
#include <Seed.hpp>
#include <QStringList>

namespace nand_build {
  struct Options {
    inspect::BootPolicy boot_policy = inspect::BootPolicy::Stock;
    QMap<QString, QString> setting_overrides;
  };

  struct Arguments {
    Options options;
    QStringList positionals;
    bool candidate_only = false;
  };

  struct Profile {
    QString area;
    QByteArray settings;
    QList<quint16> bad_blocks;
  };

  auto parse_arguments(QStringList args, bool allow_candidate_only = false) -> Arguments;
  auto boot_policy_name(inspect::BootPolicy policy) -> QString;
  auto resolve_profile(const seed::Components& seed, const Options& options) -> Profile;
  void create(const QString& seed_path, const QString& output, const Options& options);
} // namespace nand_build
