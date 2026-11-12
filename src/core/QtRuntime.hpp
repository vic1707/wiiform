// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QCoreApplication>

#include <memory>

namespace wiiform::core::detail {
  // The command line programs used to provide this runtime. Keep it alive for
  // direct library users, and reuse an application owned by the caller.
  inline auto ensure_qcore_application() -> QCoreApplication& {
    static std::unique_ptr<QCoreApplication> owned_application;
    if (auto* application = QCoreApplication::instance()) return *application;

    static int argc = 1;
    static char application_name[] = "wiiform";
    static char* argv[] = {application_name, nullptr};
    owned_application = std::make_unique<QCoreApplication>(argc, argv);
    return *owned_application;
  }
}
