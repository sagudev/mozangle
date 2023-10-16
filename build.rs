#![allow(non_upper_case_globals)]

extern crate cc;
#[cfg(feature = "egl")]
extern crate gl_generator;
extern crate walkdir;

extern crate bindgen;

use std::env;
#[cfg(feature = "egl")]
use std::path::Path;
use std::path::PathBuf;

use bindgen::Formatter;

mod build_data;

fn main() {
    let target = env::var("TARGET").unwrap();
    let egl = env::var("CARGO_FEATURE_EGL").is_ok() && target.contains("windows");

    if cfg!(feature = "egl") && !target.contains("windows") {
        panic!("Do not know how to build EGL support for a non-Windows platform.");
    }

    if cfg!(feature = "build_dlls") && !target.contains("windows") {
        panic!("Do not know how to build DLLs for a non-Windows platform.");
    }

    build_translator(&target);

    #[cfg(feature = "egl")]
    {
        build_win_statik(&target, &build_data::GLESv2, "GLESv2");
        build_win_statik(&target, &build_data::EGL, "EGL");
        generate_gl_bindings();
    }

    #[cfg(feature = "build_dlls")]
    {
        build_windows_dll(
            &build_data::EGL,
            "EGL",
            "gfx/angle/checkout/src/libEGL/libEGL_autogen.def",
        );
        build_windows_dll(
            &build_data::GLESv2,
            "GLESv2",
            "gfx/angle/checkout/src/libGLESv2/libGLESv2_autogen.def",
        );
    }
}

fn linker() -> String {
    fn exec_exists(s: &str) -> bool {
        match std::process::Command::new(s).spawn() {
            Ok(_) => true,
            Err(e) => {
                if let std::io::ErrorKind::NotFound = e.kind() {
                    return false;
                } else {
                    true
                }
            }
        }
    }
    if let Ok(linker) = env::var("LINKER") {
        linker
    } else if let Ok(linker) = env::var("RUSTC_LINKER") {
        linker
    } else if let Some(linker) =
        cc::windows_registry::find(&env::var("TARGET").unwrap(), "link.exe")
    {
        linker.get_program().to_str().unwrap().to_owned()
    } else if exec_exists("link") {
        "link".to_string()
    } else if exec_exists("lld-link") {
        "link".to_string()
    } else {
        panic!("Linker not found!");
    }
}

#[cfg(feature = "build_dlls")]
fn build_windows_dll(data: &build_data::Data, name: &str, def_file: &str) {
    println!("build_windows_dll: {name}");

    let mut cmd = std::process::Command::new(linker());
    let out_string = env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_string);

    // generate dll from statik
    //cmd.arg("/MACHINE:X86");
    cmd.arg("/dll");
    cmd.arg(format!("/DEF:{def_file}"));
    for lib in build_data::EGL
        .os_libs
        .iter()
        .chain(build_data::GLESv2.os_libs)
    {
        cmd.arg(&format!("{}.lib", lib));
    }
    cmd.arg(out_path.join(format!("EGL.lib")));
    cmd.arg(out_path.join(format!("GLESv2.lib")));
    cmd.arg(format!("/OUT:lib{name}.dll"));

    println!("{:?}", cmd);

    let status = cmd.status();
    assert!(status.unwrap().success());
}

#[cfg(feature = "egl")]
fn build_win_statik(target: &str, data: &build_data::Data, name: &str) {
    println!("build_win_statik: {name}");
    let mut build = cc::Build::new();
    build.cpp(true);
    build.std("c++17");
    for &(k, v) in data.defines {
        build.define(k, v);
    }

    if cfg!(feature = "build_dlls") {
        build.define("ANGLE_USE_EGL_LOADER", None);
    }

    for file in data.includes {
        build.include(fixup_path(file));
    }

    for file in data.sources {
        build.file(fixup_path(file));
    }

    // add zlib from libz-sys to include path
    if let Ok(zlib_include_dir) = env::var("DEP_Z_INCLUDE") {
        build.include(zlib_include_dir.replace("\\", "/"));
    }

    for lib in data.os_libs {
        println!("cargo:rustc-link-lib={}", lib);
    }

    if target.contains("x86_64") || target.contains("i686") {
        build
            .flag_if_supported("-msse2") // GNU
            .flag_if_supported("-arch:SSE2"); // MSVC
    }

    build
        .flag_if_supported("/wd4100")
        .flag_if_supported("/wd4127")
        .flag_if_supported("/wd9002");

    // Enable multiprocessing for faster builds.
    build.flag_if_supported("/MP");

    build.link_lib_modifier("-whole-archive");

    // Build lib.
    build.compile(name);
}

fn build_translator(target: &String) {
    println!("build_translator");
    let data = build_data::TRANSLATOR;

    let repo = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    env::set_current_dir(repo).unwrap();

    // common clang args
    let mut clang_args = vec![];

    for &(k, v) in data.defines {
        if let Some(v) = v {
            clang_args.push(format!("-D{}={}", k, v));
        } else {
            clang_args.push(format!("-D{}", k));
        }
    }

    for file in data.includes {
        clang_args.push(String::from("-I"));
        clang_args.push(fixup_path(file));
    }

    // Change to one of the directory that contains moz.build
    let mut build = cc::Build::new();

    for file in data.sources {
        build.file(fixup_path(file));
    }

    // Hard-code lines like `if CONFIG['OS_ARCH'] == 'Darwin':` in moz.build files
    for &(os, sources) in &[
        (
            "darwin",
            &[
                "gfx/angle/checkout/src/common/system_utils_mac.cpp",
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

    for flag in &clang_args {
        build.flag(flag);
    }

    build
        .file("src/shaders/glslang-c.cpp")
        .cpp(true)
        .std("c++17")
        .warnings(false)
        .flag_if_supported("/wd4100")
        .flag_if_supported("/wd4127")
        .flag_if_supported("/wd9002");

    if target.contains("x86_64") || target.contains("i686") {
        build
            .flag_if_supported("-msse2") // GNU
            .flag_if_supported("-arch:SSE2"); // MSVC
    }

    build.link_lib_modifier("-whole-archive");

    build.compile("translator");

    // now generate bindings
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let mut builder = bindgen::builder()
        .rust_target(bindgen::RustTarget::Stable_1_59)
        .header("./src/shaders/glslang-c.cpp")
        .opaque_type("std.*")
        .allowlist_type("Sh.*")
        .allowlist_var("SH.*")
        .rustified_enum("Sh.*")
        .formatter(Formatter::Rustfmt)
        .clang_args(["-I", "gfx/angle/checkout/include"])
        .clang_args(clang_args)
        // ensure cxx
        .clang_arg("-x")
        .clang_arg("c++")
        .clang_arg("-std=c++17");

    if target.contains("x86_64") || target.contains("i686") {
        builder = builder.clang_arg("-msse2")
    }

    for func in ALLOWLIST_FN {
        builder = builder.allowlist_function(func)
    }

    builder
        .generate()
        .expect("Should generate shader bindings")
        .write_to_file(out_dir.join("angle_bindings.rs"))
        .expect("Should write bindings to file");

    for lib in data.os_libs {
        println!("cargo:rustc-link-lib={}", lib);
    }
    println!("cargo:rerun-if-changed=src/shaders/glslang-c.cpp");
    for entry in walkdir::WalkDir::new("gfx") {
        let entry = entry.unwrap();
        println!(
            "{}",
            format!("cargo:rerun-if-changed={}", entry.path().display())
        );
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

fn fixup_path(path: &str) -> String {
    let prefix = "../../";
    assert!(path.starts_with(prefix));
    format!("gfx/angle/{}", &path[prefix.len()..])
}

#[cfg(feature = "egl")]
fn generate_gl_bindings() {
    println!("generate_gl_bindings");
    use gl_generator::{Api, Fallbacks, Profile, Registry};
    use std::fs::File;

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    let mut file = File::create(&out_dir.join("egl_bindings.rs")).unwrap();
    Registry::new(
        Api::Egl,
        (1, 5),
        Profile::Core,
        Fallbacks::All,
        [
            "EGL_ANGLE_device_d3d",
            "EGL_EXT_platform_base",
            "EGL_EXT_platform_device",
            "EGL_KHR_create_context",
            "EGL_EXT_create_context_robustness",
            "EGL_KHR_create_context_no_error",
        ],
    )
    .write_bindings(gl_generator::StaticGenerator, &mut file)
    .unwrap();

    let mut file = File::create(&out_dir.join("gles_bindings.rs")).unwrap();
    Registry::new(Api::Gles2, (2, 0), Profile::Core, Fallbacks::None, [])
        .write_bindings(gl_generator::StaticGenerator, &mut file)
        .unwrap();
}
