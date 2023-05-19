#[cfg(not(any(feature = "pio", feature = "native")))]
compile_error!("One of the features `pio` or `native` must be selected.");
use std::iter::once;

use crate::native::cargo_driver::chip::Chip;
use anyhow::*;
use bindgen::callbacks::{IntKind, ParseCallbacks};
use common::*;
use embuild::bindgen::BindgenExt;
use embuild::utils::OsStrExt;
use embuild::{bindgen as bindgen_utils, build, cargo, kconfig, path_buf};
use std::fs::File;
use std::io::BufReader;
use std::io::{BufRead, Write};
use std::str::FromStr;

mod common;
mod config;

#[cfg(feature = "native")]
mod native;
#[cfg(feature = "pio")]
mod pio;

// Note that the first alias must exclude the `pio` feature, so that in the event both
// features are specified the `pio` build driver is preferred.
// The `native` and `pio` features are really mutually exclusive but that would require
// that all dependencies specify the same feature so instead we prefer the `pio` feature
// over `native` so that if one package specifies it, this overrides the `native` feature
// for all other dependencies too.
// See https://doc.rust-lang.org/cargo/reference/features.html#mutually-exclusive-features.
#[cfg(all(feature = "native", not(feature = "pio")))]
use native as build_driver;
#[cfg(feature = "pio")]
use pio as build_driver;

#[derive(Debug)]
struct BindgenCallbacks;

impl ParseCallbacks for BindgenCallbacks {
    fn int_macro(&self, name: &str, _value: i64) -> Option<IntKind> {
        // Make sure the ESP_ERR_*, ESP_OK and ESP_FAIL macros are all i32.
        const PREFIX: &str = "ESP_";
        const SUFFIX: &str = "ERR_";
        const SUFFIX_SPECIAL: [&str; 2] = ["OK", "FAIL"];

        let name = name.strip_prefix(PREFIX)?;
        if name.starts_with(SUFFIX) || SUFFIX_SPECIAL.iter().any(|&s| name == s) {
            Some(IntKind::I32)
        } else {
            None
        }
    }
}

const STATIC_INLINE: &str = "static_inlines";

fn static_inlines_c() -> String {
    format!("{}.c", STATIC_INLINE)
}

fn static_inlines_o() -> String {
    format!("{}.o", STATIC_INLINE)
}

fn static_inlines_tmp() -> String {
    format!("{}_tmp.c", STATIC_INLINE)
}

fn static_inlines_a() -> String {
    format!("lib{}.a", STATIC_INLINE)
}

// TODO: The symbols from the components/esp_rom/<mcu>/ld are hard coded
// addresses resolved during link time, rust linker cant find those symbols
// and hence the inlines that depend on those dont work. Ignore them for now
const IGNORE_STATIC_INLINES: [&str; 3] = [
    "_xtos_interrupt_enable__extern",
    "_xtos_interrupt_disable__extern",
    "esp_cpu_intr_get_handler_arg__extern",
];

fn strip_quotes(args: &str) -> Vec<String> {
    let mut out = vec![];
    for arg in args.split_whitespace() {
        let mut chars = arg.chars();
        let first = chars.next();
        chars.next_back();
        let trim = if first == Some('\"') {
            chars.as_str()
        } else {
            arg
        };
        out.push(trim.to_string());
    }
    out
}

fn ignore_api(api: &str) -> bool {
    for ignore in IGNORE_STATIC_INLINES.iter() {
        if api.contains(ignore) {
            return true;
        }
    }
    false
}

fn process_static_inlines(
    build_output_args: &str,
    clang_args: Vec<String>,
    mcu: &str,
    headers: Vec<std::path::PathBuf>,
) -> anyhow::Result<()> {
    let chip = Chip::from_str(mcu)?;
    let gcc = format!("{}-gcc", chip.gcc_toolchain());
    let ar = format!("{}-gcc-ar", chip.gcc_toolchain());

    let out_dir_path = cargo::out_dir();
    let file = File::open(out_dir_path.join(static_inlines_c())).unwrap();
    let mut tmp = File::create(out_dir_path.join(static_inlines_tmp())).unwrap();
    let lines = BufReader::new(file).lines();
    for line in lines {
        let line = line.unwrap();
        if !ignore_api(&line) {
            tmp.write_all(line.as_bytes())?;
            writeln!(tmp)?;
        }
    }
    tmp.flush()?;

    let mut gcc_cmd = std::process::Command::new(gcc);
    let mut gcc_args = gcc_cmd
        .arg("-mlongcalls")
        .arg("-O")
        .arg("-c")
        .arg("-o")
        .arg(out_dir_path.join(&static_inlines_o()))
        .arg(out_dir_path.join(&static_inlines_tmp()));
    for hdr in headers.iter() {
        gcc_args = gcc_args.arg("-include").arg(hdr);
    }
    gcc_args = gcc_args.args(strip_quotes(build_output_args));
    gcc_args = gcc_args.args(clang_args);

    let gcc_output = gcc_args.output().unwrap();
    if !gcc_output.status.success() {
        panic!(
            "Could not compile object file:\n{}",
            String::from_utf8_lossy(&gcc_output.stderr)
        );
    }

    #[cfg(not(target_os = "windows"))]
    let lib_output = std::process::Command::new(ar)
        .arg("rcs")
        .arg(out_dir_path.join(static_inlines_a()))
        .arg(out_dir_path.join(static_inlines_o()))
        .output()
        .unwrap();
    #[cfg(target_os = "windows")]
    let lib_output = std::process::Command::new("lib")
        .arg(&out_dir_path.join(static_inlines_o()))
        .output()
        .unwrap();

    if !lib_output.status.success() {
        panic!(
            "Could not emit library file:\n{}",
            String::from_utf8_lossy(&lib_output.stderr)
        );
    }

    println!(
        "cargo:rustc-link-search=native={}",
        out_dir_path.to_string_lossy()
    );
    println!("cargo:rustc-link-lib=static={}", STATIC_INLINE);

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let build_output = build_driver::build()?;

    // We need to restrict the kconfig parameters which are turned into rustc cfg items
    // because otherwise we would be hitting rustc command line restrictions on Windows
    //
    // For now, we take all tristate parameters which are set to true, as well as a few
    // selected string ones, as per below
    //
    // This might change in future
    let kconfig_str_allow = regex::Regex::new(r"IDF_TARGET")?;

    let cfg_args = build::CfgArgs {
        args: build_output
            .kconfig_args
            .filter(|(key, value)| {
                matches!(value, kconfig::Value::Tristate(kconfig::Tristate::True))
                    || kconfig_str_allow.is_match(key)
            })
            .filter_map(|(key, value)| value.to_rustc_cfg("esp_idf", key))
            .collect(),
    };

    let mcu = cfg_args.get("esp_idf_idf_target").ok_or_else(|| {
        anyhow!(
            "Failed to get IDF_TARGET from kconfig. cfgs:\n{:?}",
            cfg_args.args
        )
    })?;

    let manifest_dir = manifest_dir()?;

    let header_file = path_buf![
        &manifest_dir,
        "src",
        "include",
        if mcu == "esp8266" {
            "esp-8266-rtos-sdk"
        } else {
            "esp-idf"
        },
        "bindings.h"
    ];

    cargo::track_file(&header_file);

    // Because we have multiple bindgen invocations and we can't clone a bindgen::Builder,
    // we have to set the options every time.
    let configure_bindgen = |bindgen: bindgen::Builder| {
        let mut outdir = cargo::out_dir();
        outdir.push(STATIC_INLINE);
        Ok(bindgen
            .parse_callbacks(Box::new(BindgenCallbacks))
            .use_core()
            .wrap_static_fns(true)
            .wrap_static_fns_path(outdir)
            .enable_function_attribute_detection()
            .clang_arg("-DESP_PLATFORM")
            .blocklist_function("strtold")
            .blocklist_function("_strtold_r")
            .blocklist_function("v.*printf")
            .blocklist_function("v.*scanf")
            .blocklist_function("_v.*printf_r")
            .blocklist_function("_v.*scanf_r")
            .blocklist_function("esp_log_writev")
            .clang_args(build_output.components.clang_args())
            .clang_args(vec![
                "-target",
                if mcu != "esp32" && mcu != "esp32s2" && mcu != "esp32s3" {
                    // Necessary to pass explicitly, because of https://github.com/rust-lang/rust-bindgen/issues/1555
                    "riscv32"
                } else {
                    // We don't really have a similar issue with Xtensa, but we pass it explicitly as well just in case
                    "xtensa"
                },
            ]))
    };

    let bindings_file = bindgen_utils::default_bindings_file()?;
    let bindgen_err = || {
        anyhow!(
            "failed to generate bindings in file '{}'",
            bindings_file.display()
        )
    };

    #[allow(unused_mut)]
    let mut headers = vec![header_file];

    #[cfg(all(feature = "native", not(feature = "pio")))]
    // Add additional headers from extra components.
    headers.extend(
        build_output
            .config
            .native
            .combined_bindings_headers()?
            .into_iter()
            .inspect(|h| cargo::track_file(h)),
    );

    configure_bindgen(build_output.bindgen.clone().builder()?)?
        .headers(headers.clone())?
        .generate()
        .with_context(bindgen_err)?
        .write_to_file(&bindings_file)
        .with_context(bindgen_err)?;

    // Generate bindings separately for each unique module name.
    #[cfg(all(feature = "native", not(feature = "pio")))]
    (|| {
        use std::fs;
        use std::io::{BufWriter, Write};

        let mut output_file =
            BufWriter::new(fs::File::options().append(true).open(&bindings_file)?);

        for (module_name, headers) in build_output.config.native.module_bindings_headers()? {
            let bindings = configure_bindgen(build_output.bindgen.clone().builder()?)?
                .headers(headers.into_iter().inspect(|h| cargo::track_file(h)))?
                .generate()?;

            writeln!(
                &mut output_file,
                "pub mod {module_name} {{\
                     {bindings}\
                 }}"
            )?;
        }
        Ok(())
    })()
    .with_context(bindgen_err)?;

    // Cargo fmt generated bindings.
    bindgen_utils::cargo_fmt_file(&bindings_file);

    let cfg_args = build::CfgArgs {
        args: cfg_args
            .args
            .into_iter()
            .chain(EspIdfVersion::parse(bindings_file)?.cfg_args())
            .chain(build_output.components.cfg_args())
            .chain(once(mcu.clone()))
            .collect(),
    };
    cfg_args.propagate();
    cfg_args.output();

    // In case other crates need to have access to the ESP-IDF C headers
    build_output.cincl_args.propagate();

    // In case other crates need to have access to the ESP-IDF toolchains
    if let Some(env_path) = build_output.env_path {
        cargo::set_metadata(embuild::build::ENV_PATH_VAR, env_path);
    }

    // In case other crates need to the ESP-IDF SDK
    cargo::set_metadata(
        embuild::build::ESP_IDF_PATH_VAR,
        build_output.esp_idf.try_to_str()?,
    );

    build_output.cincl_args.propagate();

    if let Some(link_args) = build_output.link_args {
        link_args.propagate();
    }

    let clang_args: Vec<String> = build_output.components.clang_args().collect();
    process_static_inlines(&build_output.cincl_args.args, clang_args, &mcu, headers)?;

    Ok(())
}
