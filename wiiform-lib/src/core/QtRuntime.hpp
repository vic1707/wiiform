// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QCoreApplication>
#include <QThread>
#include <atomic>
#include <stdexcept>

namespace wiiform::core::detail {
  inline void ensure_qcore_application() {
    auto* application = QCoreApplication::instance();
    if (!application) {
      static int argc = 1;
      static char application_name[] = "wiiform";
      static char* argv[] = {application_name, nullptr};
      // Qt's teardown relies on its static state. Keep the fallback alive until
      // process exit, where the OS reclaims it. Callers can own the application.
      static auto* const fallback = new QCoreApplication(argc, argv);
      application = fallback;
    }
    if (application->thread() != QThread::currentThread())
      throw std::runtime_error("Wiiform API calls must run on the QCoreApplication thread");
  }

  // A nested Qt event loop may call back into Wiiform on the owning thread.
  // Reject overlapping calls so re-entry cannot deadlock or change the AES key.
  class ApiCall {
  public:
    ApiCall() {
      if (active.test_and_set()) throw std::runtime_error("A Wiiform operation is already in progress");
      try {
        ensure_qcore_application();
      } catch (...) {
        active.clear();
        throw;
      }
    }

    ApiCall(const ApiCall&) = delete;
    auto operator=(const ApiCall&) -> ApiCall& = delete;
    ~ApiCall() { active.clear(); }

  private:
    inline static std::atomic_flag active;
  };
}
