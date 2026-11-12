#[cxx::bridge(namespace = "wiiform::ffi")]
mod ffi {
    unsafe extern "C++" {
        include!("wiiform/ffi.hpp");

        fn write_synthetic_nand(path: &str) -> Result<()>;
    }
}

pub use ffi::write_synthetic_nand;
