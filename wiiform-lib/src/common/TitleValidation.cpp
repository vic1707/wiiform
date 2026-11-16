// SPDX-License-Identifier: GPL-2.0-only
#include <TitleValidation.hpp>
#include <tiktmd.h>
#include <stdexcept>

bool decrypt_title(NusJob& job) {
  // WiiQt decrypts title keys with the common key only, so any other key index is refused.
  return job.data.size() > 1 && job.data[1].size() > 0x1f1 && static_cast<uchar>(job.data[1][0x1f1]) == 0 &&
         DecryptAndVerifyTitle(job);
}

auto validate_title(NusJob job, const QString& failure_message) -> ValidatedTitleJob {
  if (!decrypt_title(job)) throw std::runtime_error(failure_message.toStdString());
  return ValidatedTitleJob(std::move(job));
}
