// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <tools.h>
#include <utility>

// A title job can reach filesystem/cache mutation only after all metadata and
// decrypted content hashes have been checked.
class ValidatedTitleJob {
public:
  ValidatedTitleJob(const ValidatedTitleJob&) = delete;
  auto operator=(const ValidatedTitleJob&) -> ValidatedTitleJob& = delete;
  ValidatedTitleJob(ValidatedTitleJob&&) noexcept = default;
  auto operator=(ValidatedTitleJob&&) noexcept -> ValidatedTitleJob& = default;

  [[nodiscard]] auto get() const -> const NusJob& { return job_; }

private:
  explicit ValidatedTitleJob(NusJob job) : job_(std::move(job)) {}
  NusJob job_;

  friend auto validate_title(NusJob job, const QString& failure_message) -> ValidatedTitleJob;
};

bool check_title_metadata(const QByteArray& tmd, const QByteArray& ticket);
bool decrypt_title(NusJob& job);
auto validate_title(NusJob job, const QString& failure_message) -> ValidatedTitleJob;
