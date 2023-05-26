// Copyright 2022 Biagio Festa

use std::env;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let ls_qpack_dep_dir = PathBuf::from("deps/ls-qpack");

    cmake::Config::new(&ls_qpack_dep_dir)
        .define("LSQPACK_BIN", "OFF")
        .build();

    println!(
        "cargo:rustc-link-search=native={}",
        out_dir.join("lib").display()
    );
    println!("cargo:rustc-link-lib=static=ls-qpack");

    let builder = bindgen::Builder::default()
        .derive_debug(true)
        .derive_default(true)
        .derive_copy(false)
        .size_t_is_usize(true)
        .layout_tests(true)
        .generate_comments(false)
        .clang_arg(format!("-I{}", out_dir.join("include").display()))
        .header(
            ls_qpack_dep_dir
                .join("lsqpack.h")
                .into_os_string()
                .into_string()
                .unwrap(),
        )
        .header(
            ls_qpack_dep_dir
                .join("lsxpack_header.h")
                .into_os_string()
                .into_string()
                .unwrap(),
        );

    let bindings = builder.generate().expect("Unable to generate bindings");
    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
