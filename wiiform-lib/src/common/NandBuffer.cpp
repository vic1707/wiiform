// SPDX-License-Identifier: GPL-2.0-only
#include <NandBuffer.hpp>

#include <algorithm>
#include <cstring>
#include <limits>
#include <stdexcept>

namespace nand_buffer {
  Device::Device(std::span<const std::uint8_t> bytes)
      : bytes_(bytes.data()), writable_bytes_(nullptr), size_(0) {
    if (bytes.size() > static_cast<std::size_t>(std::numeric_limits<qint64>::max()))
      throw std::length_error("NAND buffer exceeds Qt device limits");
    size_ = static_cast<qint64>(bytes.size());
    open(QIODevice::ReadOnly | QIODevice::Unbuffered);
  }

  Device::Device(std::span<std::uint8_t> bytes)
      : bytes_(bytes.data()), writable_bytes_(bytes.data()), size_(0) {
    if (bytes.size() > static_cast<std::size_t>(std::numeric_limits<qint64>::max()))
      throw std::length_error("NAND buffer exceeds Qt device limits");
    size_ = static_cast<qint64>(bytes.size());
    open(QIODevice::ReadWrite | QIODevice::Unbuffered);
  }

  auto Device::isSequential() const -> bool { return false; }

  auto Device::size() const -> qint64 { return size_; }

  auto Device::seek(qint64 position) -> bool {
    if (position < 0 || position > size_) return false;
    return QIODevice::seek(position);
  }

  auto Device::readData(char* data, qint64 maximum) -> qint64 {
    if (maximum < 0 || pos() < 0 || pos() > size_) return -1;
    const auto available = size_ - pos();
    const auto count = std::min(maximum, available);
    if (count == 0) return 0;
    std::memcpy(data, bytes_ + pos(), static_cast<std::size_t>(count));
    return count;
  }

  auto Device::writeData(const char* data, qint64 count) -> qint64 {
    if (!writable_bytes_ || count < 0 || pos() < 0 || pos() > size_) return -1;
    const auto available = size_ - pos();
    const auto written = std::min(count, available);
    if (written == 0) return 0;
    std::memcpy(writable_bytes_ + pos(), data, static_cast<std::size_t>(written));
    return written;
  }
} // namespace nand_buffer
