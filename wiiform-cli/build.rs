// SPDX-License-Identifier: GPL-2.0-only
use std::{env, path::PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let project = manifest.parent().unwrap();
    let headers = project.join("src-wiiformlib/include");
    let native = project.join("build/lib");
    let (extension, loader) = match env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("macos") => ("dylib", "@executable_path"),
        Ok("linux") => ("so", "$ORIGIN"),
        _ => panic!("Wiiform supports macOS and Linux"),
    };
    let library = native.join(format!("libwiiform-core.{extension}"));
    assert!(
        library.is_file(),
        "Run mise run native:build to create {}",
        library.display()
    );

    cxx_build::bridge("src/ffi.rs")
        .file("src/interop/ffi_adapter.cpp")
        .include(&headers)
        .include("src/interop")
        .std("c++20")
        .compile("wiiform-cxx-bridge");

    println!("cargo:rustc-link-search=native={}", native.display());
    println!("cargo:rustc-link-lib=dylib=wiiform-core");
    println!("cargo:rustc-link-arg-bins=-Wl,-rpath,{loader}/../lib");
    if extension == "dylib" {
        println!("cargo:rustc-link-arg-bins=-Wl,-headerpad_max_install_names");
    }
    for path in [
        "src/ffi.rs",
        "src/interop/ffi_adapter.cpp",
        "src/interop/ffi.hpp",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rerun-if-changed={}", headers.display());
    println!("cargo:rerun-if-changed={}", library.display());
}
