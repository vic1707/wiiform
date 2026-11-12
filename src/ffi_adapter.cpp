#include <wiiform/core.hpp>
#include <wiiform/ffi.hpp>

namespace wiiform::ffi {

void write_synthetic_nand(rust::Str path) {
  wiiform::core::write_synthetic_nand(std::string_view(path.data(), path.size()));
}

} // namespace wiiform::ffi
