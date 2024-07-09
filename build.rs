#![allow(non_upper_case_globals)]

extern crate cc;
#[cfg(feature = "egl")]
extern crate gl_generator;
extern crate walkdir;

extern crate bindgen;

use std::collections::HashSet;
use std::env;
use std::path::PathBuf;

use bindgen::Formatter;
use build_data::Data;

use crate::build_data::Libs;

mod build_data;

fn main() {
    let target = env::var("TARGET").unwrap();

    let data = vec![
        build_data::ANGLE_COMMON,
        build_data::PREPROCESSOR,
        build_data::TRANSLATOR,
    ];

    let includes: Vec<String> = data
        .iter()
        .flat_map(|d| d.includes)
        .map(|p| fixup_path(p))
        .collect();
    let sources: Vec<String> = data
        .iter()
        .flat_map(|d| d.sources)
        .map(|p| fixup_path(p))
        .collect();
    let defines = build_data::TRANSLATOR.defines;

    let mut build = cc::Build::new();
    build.cpp(true).std("c++17").warnings(false);
    build.includes(includes);
    build.files(sources);

    // These platform-specific files are added conditionally in moz.build files
    // `if CONFIG['OS_ARCH'] == 'Darwin':`
    for &(os, sources) in &[
        (
            "darwin",
            &[
                "gfx/angle/checkout/src/common/system_utils_mac.cpp",
                "gfx/angle/checkout/src/common/system_utils_apple.cpp",
                "gfx/angle/checkout/src/common/system_utils_posix.cpp",
            ][..],
        ),
        (
            "linux",
            &[
                "gfx/angle/checkout/src/common/system_utils_linux.cpp",
                "gfx/angle/checkout/src/common/system_utils_posix.cpp",
            ][..],
        ),
        (
            "windows",
            &[
                "gfx/angle/checkout/src/common/system_utils_win.cpp",
                "gfx/angle/checkout/src/common/system_utils_win32.cpp",
            ][..],
        ),
    ] {
        if target.contains(os) {
            for source in sources {
                build.file(source);
            }
            break;
        }
    }

    build
        .flag_if_supported("/wd4100")
        .flag_if_supported("/wd4127")
        .flag_if_supported("/wd9002");

    if target.contains("x86_64") || target.contains("i686") {
        build
            .flag_if_supported("-msse2") // GNU
            .flag_if_supported("-arch:SSE2"); // MSVC
    }

    // Enable multiprocessing for faster builds.
    build.flag_if_supported("/MP");

    for (k, v) in defines {
        build.define(k, *v);
    }

    build.file("src/shaders/glslang-c.cpp");

    build.compile("glslang");

    let mut builder = bindgen::builder()
        .rust_target(bindgen::RustTarget::Stable_1_59)
        .header("./src/shaders/glslang-c.cpp")
        .opaque_type("std.*")
        .allowlist_type("Sh.*")
        .allowlist_var("SH.*")
        .rustified_enum("Sh.*")
        .formatter(Formatter::Rustfmt)
        .clang_args(["-I", "./gfx/angle/checkout/include"])
        // ensure cxx
        .clang_arg("-x")
        .clang_arg("c++")
        .clang_arg("-std=c++17");

    for include in build_data::ANGLE_COMMON.includes {
        builder = builder.clang_args(["-I", &fixup_path(include)]);
    }

    for func in ALLOWLIST_FN {
        builder = builder.allowlist_function(func)
    }

    builder
        .generate()
        .expect("Should generate shader bindings")
        .write_to_file(
            PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("glslang_glue_bindings.rs"),
        )
        .expect("Should write bindings to file");

    println!("cargo:rerun-if-changed=src/shaders/glslang-c.cpp");

    for entry in walkdir::WalkDir::new("gfx") {
        let entry = entry.unwrap();
        println!("cargo:rerun-if-changed={}", entry.path().display());
    }
}

const ALLOWLIST_FN: &'static [&'static str] = &[
    "GLSLangInitialize",
    "GLSLangFinalize",
    "GLSLangInitBuiltInResources",
    "GLSLangConstructCompiler",
    "GLSLangDestructCompiler",
    "GLSLangCompile",
    "GLSLangClearResults",
    "GLSLangGetShaderVersion",
    "GLSLangGetShaderOutputType",
    "GLSLangGetObjectCode",
    "GLSLangGetInfoLog",
    "GLSLangIterUniformNameMapping",
    "GLSLangGetNumUnpackedVaryingVectors",
];

/// Make a path relative to the working directory that is used for the build.
fn fixup_path(path: &str) -> String {
    let prefix = "../../";
    assert!(path.starts_with(prefix));
    format!("gfx/angle/{}", &path[prefix.len()..])
}
