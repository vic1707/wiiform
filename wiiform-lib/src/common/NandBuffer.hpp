// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QIODevice>

#include <cstdint>
#include <span>

namespace nand_buffer {
  // Seekable view over caller-owned NAND bytes. The caller must keep the span
  // alive until every NandBin using this device has been destroyed.
  class Device final : public QIODevice {
  public:
    explicit Device(std::span<const std::uint8_t> bytes);
    explicit Device(std::span<std::uint8_t> bytes);

    [[nodiscard]] auto isSequential() const -> bool override;
    [[nodiscard]] auto size() const -> qint64 override;
    [[nodiscard]] auto seek(qint64 position) -> bool override;

  protected:
    auto readData(char* data, qint64 maximum) -> qint64 override;
    auto writeData(const char* data, qint64 size) -> qint64 override;

  private:
    const std::uint8_t* bytes_;
    std::uint8_t* writable_bytes_;
    qint64 size_;
  };
} // namespace nand_buffer
