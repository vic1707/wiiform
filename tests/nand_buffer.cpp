// SPDX-License-Identifier: GPL-2.0-only
#include <NandBuffer.hpp>
#include <nandbin.h>
#include <tools.h>

#include <QCoreApplication>

#include <cstdint>
#include <iostream>
#include <stdexcept>
#include <vector>

namespace {
  void check(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }
}

int main(int argc, char** argv) {
  QCoreApplication app(argc, argv);
  try {
    constexpr std::size_t image_size = 0x21000400;
    std::vector<std::uint8_t> image(image_size, 0);
    QByteArray keys(0x400, '\0');
    QByteArray boot_blocks(0x108000, '\xff');
    const QByteArray expected("buffer-backed NAND test payload");

    {
      nand_buffer::Device device{std::span<std::uint8_t>(image)};
      NandBin nand;
      check(nand.CreateNew(&device, keys, boot_blocks), "Cannot initialize NAND in caller buffer");
      check(nand.SetDevice(&device) && nand.InitNand(), "Cannot authenticate buffer-backed NAND");
      const auto file = nand.CreateEntry("/buffer-test", 0x1000, 1, NAND_FILE, NAND_RW, NAND_RW, NAND_RW);
      check(file != 0 && nand.SetData(file, expected) && nand.WriteMetaData(),
            "Cannot write file and metadata into caller buffer");
      check(nand.CheckHmacMeta(0x7ff0), "Buffer NAND metadata HMAC did not authenticate");
      check(nand.CheckHmacData(file), "Buffer NAND file HMAC did not authenticate");
      check(nand.GetData("/buffer-test") == expected, "Buffer NAND readback differs from the written file");
    }

    {
      nand_buffer::Device device{std::span<const std::uint8_t>(image)};
      NandBin nand;
      check(nand.SetDevice(&device) && nand.InitNand(), "Cannot reopen read-only buffer NAND");
      check(nand.GetData("/buffer-test") == expected, "Read-only buffer does not see the written bytes");
      const auto before = image.front();
      check(device.write("x", 1) == -1, "Read-only NAND buffer accepted a write");
      check(image.front() == before, "Rejected buffer write changed the NAND");
    }

    std::cout << "Buffer-backed NAND creation, authentication, write, and readback passed\n";
    return 0;
  } catch (const std::exception& error) {
    std::cerr << error.what() << '\n';
    return 1;
  }
}
