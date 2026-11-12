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

    cxx_build::bridge("src/ffi.rs")
        .file("src/ffi_adapter.cpp")
        .include("include")
        .std("c++20")
        .compile("wiiform-cxx-bridge");

    println!("cargo:rustc-link-search=native={}", native.display());
    println!("cargo:rustc-link-lib=dylib=wiiform-core-write");
    println!("cargo:rustc-link-arg-bins=-Wl,-rpath,{loader}/../lib");
    if extension == "dylib" {
        println!("cargo:rustc-link-arg-bins=-Wl,-headerpad_max_install_names");
    }
    println!("cargo:rerun-if-env-changed=WIIFORM_NATIVE_LIB_DIR");
    for path in [
        "src/ffi.rs",
        "src/ffi_adapter.cpp",
        "include/wiiform/write.hpp",
        "include/wiiform/types.hpp",
        "include/wiiform/ffi.hpp",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rerun-if-changed={}", library.display());
}
