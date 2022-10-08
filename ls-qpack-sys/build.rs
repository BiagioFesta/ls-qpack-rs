// Copyright 2022 Biagio Festa

use std::env;
use std::path::PathBuf;

fn main() {
    let ls_qpack_dep_dir = PathBuf::from("deps/ls-qpack");

    let ls_qpack_build_dir = cmake::Config::new(&ls_qpack_dep_dir)
        .build_target("ls-qpack")
        .build()
        .join("build");

    println!(
        "cargo:rustc-link-search=native={}",
        ls_qpack_build_dir.display()
    );
    println!("cargo:rustc-link-lib=static=ls-qpack");

    let builder = bindgen::Builder::default()
        .derive_debug(true)
        .derive_default(true)
        .derive_copy(false)
        .size_t_is_usize(true)
        .layout_tests(true)
        .generate_comments(false)
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
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
