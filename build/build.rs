#[cfg(not(any(feature = "pio", feature = "native")))]
compile_error!("One of the features `pio` or `native` must be selected.");
use std::iter::once;

use anyhow::*;
use bindgen::callbacks::{IntKind, ParseCallbacks};
use common::*;
use embuild::bindgen::BindgenExt;
use embuild::utils::OsStrExt;
use embuild::{bindgen as bindgen_utils, build, cargo, kconfig, path_buf};

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
        Ok(bindgen
            .parse_callbacks(Box::new(BindgenCallbacks))
            .use_core()
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
                if mcu == "esp32c3" {
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
        .headers(headers)?
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
            .chain(once(mcu))
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

    Ok(())
}
