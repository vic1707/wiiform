// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QByteArray>
#include <QMap>
#include <QString>
#include <yaml-cpp/yaml.h>

namespace seed {
  struct Components {
    YAML::Node manifest;
    QMap<QString, QByteArray> files;
  };

  void export_backup(const QString& backup, const QString& destination);
  auto read(const QString& input) -> Components;
  auto unpack(const QString& input, const QString& destination) -> YAML::Node;
  auto encode_settings(const YAML::Node& settings) -> QByteArray;
  auto settings_fields(const YAML::Node& manifest) -> YAML::Node;
  auto has_original_settings(const YAML::Node& manifest) -> bool;
} // namespace seed
