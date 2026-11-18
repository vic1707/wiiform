// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <TitleValidation.hpp>

#include <QByteArray>
#include <QList>

// Rebuilding a title from a stock one: new content list, new title ID and
// version, fakesigned. Used for the IOS wiiform derives (RVLoader, d2x cIOS).
namespace title_build {
  constexpr quint16 normal_content = 0x0001;
  constexpr quint16 shared_content = 0x8001;

  struct Content {
    quint32 cid;
    quint16 type;
    QByteArray bytes;
  };

  // The stock TMD's header with these contents, numbered by position.
  auto tmd(const QByteArray& stock_tmd, const QList<Content>& contents, quint16 boot_position) -> QByteArray;
  // Sets the title ID and version of `metadata` and fakesigns it.
  auto retitle_tmd(const QByteArray& metadata, quint64 title_id, quint16 version) -> QByteArray;
  // Moves `ticket` to `title_id` (re-encrypting its title key) and fakesigns it.
  auto retitle_ticket(const QByteArray& ticket, quint64 title_id) -> QByteArray;
  // The finished title, checked like any other before it is installed.
  auto job(quint64 title_id, quint16 version, const QByteArray& metadata, const QByteArray& ticket,
           const QList<Content>& contents, const QString& failure) -> ValidatedTitleJob;
}
