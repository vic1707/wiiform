// SPDX-License-Identifier: GPL-2.0-only
#include <PriiloaderSettings.hpp>

#include <QtEndian>
#include <stdexcept>

namespace {
  template<class Enum>
  auto setting_byte(Enum value, Enum last) -> char {
    const auto byte = static_cast<std::uint8_t>(value);
    if (byte > static_cast<std::uint8_t>(last))
      throw std::runtime_error("Unsupported Priiloader setting value");
    return static_cast<char>(byte);
  }
}

namespace priiloader {
  auto encode_settings(const wiiform::PriiloaderSettings& settings) -> QByteArray {
    using namespace wiiform;
    // Priiloader tag 0.10.0 (2b70b36), Settings in settings.h and version.h.
    QByteArray bytes(32, '\0');
    bytes[0] = setting_byte(settings.autoboot, Autoboot::file);
    qToBigEndian<quint32>(0x000a0000, reinterpret_cast<uchar*>(bytes.data() + 4));
    bytes[8] = setting_byte(settings.return_to, ReturnTo::file);
    bytes[9] = setting_byte(settings.shutdown_to, ShutdownTo::autoboot);
    bytes[10] = settings.stop_disc;
    bytes[11] = settings.lid_slot_on_error;
    bytes[12] = settings.ignore_shutdown_mode;
    bytes[21] = 1; // Use the System Menu IOS.
    bytes[22] = settings.black_background;
    bytes[23] = settings.dump_gecko_text;
    bytes[26] = settings.show_rc_updates;
    bytes[27] = setting_byte(settings.preferred_mount_point, MountPoint::usb);
    return bytes;
  }
}
