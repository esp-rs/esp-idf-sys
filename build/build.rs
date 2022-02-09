#[cfg(not(any(feature = "pio", feature = "native")))]
compile_error!("One of the features `pio` or `native` must be selected.");

use std::env;
use std::iter::once;
use std::path::PathBuf;
use std::collections::{HashMap, HashSet};

use ::bindgen::callbacks::{IntKind, ParseCallbacks};
use anyhow::*;
use common::*;
use embuild::utils::OsStrExt;
use embuild::{bindgen, build, cargo, kconfig, path_buf};

mod common;

// Note that the feature `native` must come before `pio`. These features are really
// mutually exclusive but that would require that all dependencies specify the same
// feature so instead we prefer the `native` feature over `pio` so that if one package
// specifies it, this overrides the `pio` feature for all other dependencies too.
// See https://doc.rust-lang.org/cargo/reference/features.html#mutually-exclusive-features.
#[cfg(any(feature = "pio", feature = "native"))]
#[cfg_attr(feature = "native", path = "native.rs")]
#[cfg_attr(all(feature = "pio", not(feature = "native")), path = "pio.rs")]
mod build_driver;

#[derive(Debug)]
struct BindgenCallbacks;

impl ParseCallbacks for BindgenCallbacks {
    fn item_name(&self, original_item_name: &str) -> Option<String> {
        if original_item_name == "mbedtls_time_t" {
            // TODO better fix for this
            return None;
        }
        Some(
            original_item_name
                .trim_start_matches("mbedtls_")
                .trim_start_matches("MBEDTLS_")
                .to_owned(),
        )
    }

    fn enum_variant_name(
        &self,
        _enum_name: Option<&str>,
        original_variant_name: &str,
        _variant_value: ::bindgen::callbacks::EnumVariantValue,
    ) -> Option<String> {
        self.item_name(original_variant_name)
    }

    // fn int_macro(&self, name: &str, value: i64) -> Option<IntKind> {
    //     // Make sure the ESP_ERR_*, ESP_OK and ESP_FAIL macros are all i32.
    //     const PREFIX: &str = "ESP_";
    //     const SUFFIX: &str = "ERR_";
    //     const SUFFIX_SPECIAL: [&str; 2] = ["OK", "FAIL"];

    //     let name = name.strip_prefix(PREFIX)?;

    //     // if name.starts_with("mbedtls") || name.starts_with("MBEDTLS") {
    //         // TODO remove this? changes types to c_int etc, maybe we should only do this for mbedtls types?
    //         if value < (i32::MIN as i64) || value > (i32::MAX as i64) {
    //             Some(::bindgen::callbacks::IntKind::LongLong)
    //         } else {
    //             Some(::bindgen::callbacks::IntKind::Int)
    //         }
    //     // } else if name.starts_with(SUFFIX) || SUFFIX_SPECIAL.iter().any(|&s| name == s) {
    //     //     Some(IntKind::I32)
    //     // } else {
    //     //     None
    //     // }
    // }

    fn int_macro(&self, _name: &str, value: i64) -> Option<IntKind> {
        // TODO remove this? changes types to c_int etc, maybe we should only do this for mbedtls types?
        if value < (i32::MIN as i64) || value > (i32::MAX as i64) {
            Some(IntKind::LongLong)
        } else {
            Some(IntKind::Int)
        }
    }

    // fn blocklisted_type_implements_trait(&self, _name: &str, derive_trait: bindgen::callbacks::DeriveTrait) -> Option<bindgen::callbacks::ImplementsTrait> {
    //     if derive_trait == bindgen::callbacks::DeriveTrait::Default {
    //         Some(bindgen::callbacks::ImplementsTrait::Manually)
    //     } else {
    //         Some(bindgen::callbacks::ImplementsTrait::Yes)
    //     }
    // }
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

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);

    let header_file = path_buf![
        manifest_dir,
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

    // hard coded features, derived manually from the build script inside
    let platform_components: HashMap<&'static str, HashSet<&'static str>> = [
        ("threading", ["pthread"].into()),
        ("std", ["net", "entropy", "fs"].into()),
        ("time", ["libc"].into()), /* we might need custom time instead */
    ]
    .into();

    for (feature, components) in &platform_components {
        for component in components {
            println!(r#"cargo:rustc-cfg={}_component="{}""#, feature, component);
        }
    }
    // TODO this may need to be a hard coded env variable DEP_MBEDTLS_PLATFORM_COMPONENTS
    // at the moment I _think_ its emitting DEP_ESPIDF_PLATFORM_COMPONENTS or soomething like that
    // "cargo:rustc-env=DEP_MBEDTLS_PLATFORM_COMPONENTS={}",
    println!(
        "cargo:platform-components={}",
        platform_components
            .iter()
            .flat_map(|(feature, components)| {
                components
                    .iter()
                    .map(move |component| format!(r#"{}_component={}"#, feature, component))
            })
            .collect::<Vec<_>>()
            .join(",")
    );

    let bindings_file = bindgen::run(
        build_output
            .bindgen
            .builder()?
            .parse_callbacks(Box::new(BindgenCallbacks))
            .ctypes_prefix("c_types")
            .header(header_file.try_to_str()?)
            .blocklist_function("strtold")
            .blocklist_function("_strtold_r")
            .size_t_is_usize(true) /* mbedtls requires size_t to be usize, default def is u32. */
            .prepend_enum_name(false)
            .translate_enum_integer_types(true)
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
            ]),
    )?;

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
        // TODO: Replace with embuild::build::VAR_ENV_PATH once we have a new embuild release
        cargo::set_metadata("EMBUILD_ENV_PATH", env_path);
    }

    // In case other crates need to the ESP-IDF SDK
    // TODO: Replace with embuild::espidf::XXX paths once we have a new embuild release
    cargo::set_metadata("EMBUILD_ESP_IDF_PATH", build_output.esp_idf.try_to_str()?);

    build_output.cincl_args.propagate();

    if let Some(link_args) = build_output.link_args {
        link_args.propagate();
    }

    Ok(())
}
