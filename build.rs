// SPDX-License-Identifier: GPL-2.0-only
use std::{env, path::PathBuf};

fn main() {
    let native = PathBuf::from(
        env::var_os("WIIFORM_NATIVE_LIB_DIR")
            .expect("build the Xmake core and set WIIFORM_NATIVE_LIB_DIR to build/lib"),
    );
    let (extension, loader) = match env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("macos") => ("dylib", "@executable_path"),
        Ok("linux") => ("so", "$ORIGIN"),
        _ => panic!("Wiiform supports macOS and Linux"),
    };
    let library = native.join(format!("libwiiform-core-write.{extension}"));
    assert!(
        library.is_file(),
        "missing native core at {}",
        library.display()
    );

    cxx_build::bridge("rust/ffi.rs")
        .file("rust/interop/ffi_adapter.cpp")
        .include("src-wiiformlib/include")
        .include("rust/interop")
        .std("c++20")
        .compile("wiiform-cxx-bridge");

    println!("cargo:rustc-link-search=native={}", native.display());
    println!("cargo:rustc-link-lib=dylib=wiiform-core-write");
    println!("cargo:rustc-link-arg-bins=-Wl,-rpath,{loader}/../lib");
    if extension == "dylib" {
        println!("cargo:rustc-link-arg-bins=-Wl,-headerpad_max_install_names");
        println!("cargo:rustc-link-arg-bin=wii-seed=-Wl,-dead_strip_dylibs");
    }
    println!("cargo:rerun-if-env-changed=WIIFORM_NATIVE_LIB_DIR");
    for path in [
        "rust/ffi.rs",
        "rust/interop/ffi_adapter.cpp",
        "src-wiiformlib/include/wiiform/write.hpp",
        "src-wiiformlib/include/wiiform/types.hpp",
        "rust/interop/ffi.hpp",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rerun-if-changed={}", library.display());
}
