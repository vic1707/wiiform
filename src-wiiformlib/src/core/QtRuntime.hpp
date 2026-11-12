// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QCoreApplication>

namespace wiiform::core::detail {
  inline auto ensure_qcore_application() -> QCoreApplication& {
    if (auto* application = QCoreApplication::instance()) return *application;

    static int argc = 1;
    static char application_name[] = "wiiform";
    static char* argv[] = {application_name, nullptr};
    // Qt's teardown relies on its static state. Keep the fallback alive until
    // process exit, where the OS reclaims it. Callers can own the application.
    static auto* const application = new QCoreApplication(argc, argv);
    return *application;
  }
}
