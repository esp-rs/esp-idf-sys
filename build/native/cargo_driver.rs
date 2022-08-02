use std::convert::TryFrom;
use std::path::Path;
use std::str::FromStr;
use std::{env, fs};

use anyhow::{anyhow, bail, Context, Error, Result};
use config::{ESP_IDF_REPOSITORY_VAR, ESP_IDF_VERSION_VAR};
use embuild::cargo::IntoWarning;
use embuild::cmake::file_api::codemodel::Language;
use embuild::cmake::file_api::ObjKind;
use embuild::espidf::{EspIdfOrigin, EspIdfRemote, FromEnvError};
use embuild::fs::copy_file_if_different;
use embuild::utils::{OsStrExt, PathExt};
use embuild::{bindgen, build, cargo, cmake, espidf, git, kconfig, path_buf};

use self::chip::Chip;
use crate::common::{
    self, list_specific_sdkconfigs, manifest_dir, to_cmake_path_list, workspace_dir,
    EspIdfBuildOutput, EspIdfComponents, InstallDir, V_4_3_2_PATCHES,
};
use crate::config::{BuildConfig, ESP_IDF_GLOB_VAR_PREFIX, ESP_IDF_TOOLS_INSTALL_DIR_VAR};

pub mod chip;
pub mod config;

pub fn build() -> Result<EspIdfBuildOutput> {
    let out_dir = cargo::out_dir();
    let target = env::var("TARGET")?;
    let workspace_dir = workspace_dir()?;
    let manifest_dir = manifest_dir()?;

    let config = BuildConfig::try_from_env().map(|mut config| {
        config.with_cargo_metadata().into_warning();
        config
    })?;
    config.print();

    let chip = if let Some(mcu) = &config.mcu {
        Chip::from_str(mcu)?
    } else {
        Chip::detect(&target)?
    };
    let chip_name = chip.to_string();
    let profile = common::build_profile();
    let cmake_generator = config.native.esp_idf_cmake_generator();

    // A closure to specify which tools `idf-tools.py` should install.
    let make_tools = move |repo: &git::Repository,
                           version: &Result<espidf::EspIdfVersion>|
          -> Result<Vec<espidf::Tools>> {
        eprintln!(
            "Using esp-idf {} at '{}'",
            espidf::EspIdfVersion::format(version),
            repo.worktree().display()
        );

        let mut tools = vec![];
        let mut subtools = vec![chip.gcc_toolchain()];

        // Use custom cmake for esp-idf<4.4, because we need at least cmake-3.20
        match version.as_ref().map(|v| (v.major, v.minor, v.patch)) {
            Ok((major, minor, _)) if major >= 4 && minor >= 4 => subtools.push("cmake"),
            _ => {
                tools.push(espidf::Tools::cmake()?);
            }
        }

        if cmake_generator == cmake::Generator::Ninja {
            subtools.push("ninja")
        }
        if !cfg!(target_os = "linux") || !cfg!(target_arch = "aarch64") {
            subtools.extend(chip.ulp_gcc_toolchain());
        }
        tools.push(espidf::Tools::new(subtools));

        Ok(tools)
    };

    // Get the install dir location from the build config, or use
    // [`crate::config::DEFAULT_TOOLS_INSTALL_DIR`] if unset.
    let (install_dir, allow_from_env) = config.esp_idf_tools_install_dir()?;
    // EspIdf must come from the environment if `esp_idf_tools_install_dir` == `fromenv`".
    let require_from_env = install_dir.is_from_env();
    let maybe_from_env = require_from_env || allow_from_env;

    // Closure to install the esp-idf using `embuild::espidf::Installer`.
    let install = |esp_idf_origin: EspIdfOrigin| -> Result<espidf::EspIdf> {
        match &esp_idf_origin {
            EspIdfOrigin::Custom(repo) => {
                eprintln!(
                    "Using custom user-supplied esp-idf repository at '{}' (detected from env variable `{}`)",
                    repo.worktree().display(),
                    espidf::IDF_PATH_VAR
                );
                if let Some(custom_url) = &config.native.esp_idf_repository {
                    cargo::print_warning(format_args!(
                        "Ignoring configuration setting `{ESP_IDF_REPOSITORY_VAR}=\"{custom_url}\"`: \
                         custom esp-idf repository detected via ${}",
                        espidf::IDF_PATH_VAR
                    ));
                }
                if let Some(custom_version) = &config.native.esp_idf_version {
                    cargo::print_warning(format_args!(
                        "Ignoring configuration setting `{ESP_IDF_VERSION_VAR}` ({custom_version}): \
                         custom esp-idf repository detected via ${}",
                        espidf::IDF_PATH_VAR
                    ));
                }
            }
            EspIdfOrigin::Managed(remote) => {
                eprintln!("Using managed esp-idf repository: {remote:?}");
            }
        };

        espidf::Installer::new(esp_idf_origin)
            .install_dir(install_dir.path().map(Into::into))
            .with_tools(make_tools)
            .install()
            .context("Could not install esp-idf")
    };

    let idf = match (espidf::EspIdf::try_from_env(), maybe_from_env) {
        (Ok(idf), true) => {
            eprintln!(
                "Using activated esp-idf {} environment at '{}'",
                espidf::EspIdfVersion::format(&idf.version),
                idf.repository.worktree().display()
            );

            idf
        },
        (Ok(idf), false) => {
                cargo::print_warning(format_args!(
                    "Ignoring activated esp-idf environment: {ESP_IDF_TOOLS_INSTALL_DIR_VAR} != {}", InstallDir::FromEnv
                ));
                install(EspIdfOrigin::Custom(idf.repository))?
        },
        (Err(FromEnvError::NotActivated { source: err, .. }), true) |
        (Err(FromEnvError::NoRepo(err)), true) if require_from_env => {
            return Err(err.context(
                format!("activated esp-idf environment not found but required by {ESP_IDF_TOOLS_INSTALL_DIR_VAR} == {install_dir}")
            ))
        }
        (Err(FromEnvError::NoRepo(_)), _) => {
            install(EspIdfOrigin::Managed(EspIdfRemote {
                git_ref: config.native.esp_idf_version(),
                repo_url: config.native.esp_idf_repository.clone()
            }))?
        },
        (Err(FromEnvError::NotActivated { esp_idf_repo, .. }), _) => {
            install(EspIdfOrigin::Custom(esp_idf_repo))?
        }
    };

    // Apply patches, only if the patches were not previously applied and if the esp-idf repo is managed.
    if idf.is_managed_espidf {
        let patch_set = match idf.version.map(|v| (v.major, v.minor, v.patch)) {
            // master branch
            _ if idf.repository.get_default_branch()? == idf.repository.get_branch_name()? => &[],
            Ok((4, 4, _)) => &[],
            Ok((4, 3, patch)) if patch > 2 => &[],
            Ok((4, 3, patch)) if patch == 2 => V_4_3_2_PATCHES,
            Ok((major, minor, patch)) => {
                cargo::print_warning(format_args!(
                    "esp-idf version ({major}.{minor}.{patch}) not officially supported by `esp-idf-sys`. \
                     Supported versions are 'master', 'release/v4.4', 'release/v4.3', 'v4.4(.X)', 'v4.3.3', 'v4.3.2'.",
                ));
                &[]
            }
            Err(err) => {
                err.context("could not determine patch-set for esp-idf repository")
                    .into_warning();
                &[]
            }
        };
        if !patch_set.is_empty() {
            idf.repository
                .apply_once(patch_set.iter().map(|p| manifest_dir.join(p)))?;
        }
    }

    env::set_var("PATH", &idf.exported_path);

    // The `kconfig.cmake` script looks at this variable if it should compile `mconf` on windows.
    // But this variable is also present when using git-bash which doesn't have gcc.
    env::remove_var("MSYSTEM");

    // Remove the sdkconfig file generated by the esp-idf so that potential changes
    // in the user provided sdkconfig and sdkconfig.defaults don't get ignored.
    // TODO: I'm really not sure why we have to do this.
    let _ = fs::remove_file(path_buf![&out_dir, "sdkconfig"]);

    // Create cmake project.
    copy_file_if_different(
        manifest_dir.join(path_buf!("resources", "cmake_project", "CMakeLists.txt")),
        &out_dir,
    )?;
    copy_file_if_different(
        manifest_dir.join(path_buf!("resources", "cmake_project", "main.c")),
        &out_dir,
    )?;

    // Copy additional globbed files specified by user env variables
    for file in build::tracked_env_globs_iter(ESP_IDF_GLOB_VAR_PREFIX)? {
        let dest_path = out_dir.join(file.1);
        fs::create_dir_all(dest_path.parent().unwrap())?;
        // TODO: Maybe warn if this overwrites a critical file (e.g. CMakeLists.txt).
        // It could be useful for the user to explicitly overwrite our files.
        copy_file_if_different(&file.0, &out_dir)?;
    }

    // Resolve the `sdkconfig` and all `sdkconfig.defaults` files specified in the build
    // config.
    let sdkconfig = {
        let file = config.esp_idf_sdkconfig();
        let path = Path::new(&file).abspath_relative_to(&workspace_dir);
        let cfg = list_specific_sdkconfigs(path, &profile, &chip_name).next();
        if let Some(ref file) = cfg {
            cargo::track_file(file);
        }
        cfg
    };

    let sdkconfig_defaults = {
        let gen_defaults_path = out_dir.join("gen-sdkconfig.defaults");
        fs::write(&gen_defaults_path, generate_sdkconfig_defaults()?)?;

        let mut result = vec![gen_defaults_path];
        result.extend(
            config
                .esp_idf_sdkconfig_defaults()
                .into_iter()
                .flat_map(|v| {
                    list_specific_sdkconfigs(
                        v.abspath_relative_to(&workspace_dir),
                        &profile,
                        &chip_name,
                    )
                    // We need to reverse the order here so that the more
                    // specific defaults come last.
                    .rev()
                    .inspect(|p| cargo::track_file(p))
                }),
        );
        result
    };

    let defaults_files = to_cmake_path_list(
        sdkconfig_defaults
            .iter()
            // Use the `sdkconfig` as a defaults file to prevent it from being changed by the
            // build. It must be the last defaults file so that its options have precendence
            // over any actual defaults from files before it.
            .chain(sdkconfig.as_ref()),
    )?;

    let cmake_toolchain_file = path_buf![
        &idf.repository.worktree(),
        "tools",
        "cmake",
        chip.cmake_toolchain_file()
    ];

    // Get the asm, C and C++ flags from the toolchain file, these would otherwise get
    // overwritten because `cmake::Config` also sets these (see
    // https://github.com/espressif/esp-idf/issues/7507).
    let (asm_flags, c_flags, cxx_flags) = {
        let extractor_script = cmake::script_variables_extractor(&cmake_toolchain_file)?;

        let output = embuild::cmd!(
            cmake::cmake(),
            "-P",
            extractor_script.as_ref().as_os_str();
            env=("IDF_PATH", &idf.repository.worktree().as_os_str()))
        .stdout()?;

        let mut vars = cmake::process_script_variables_extractor_output(output)?;
        (
            vars.remove("CMAKE_ASM_FLAGS").unwrap_or_default(),
            vars.remove("CMAKE_C_FLAGS").unwrap_or_default(),
            vars.remove("CMAKE_CXX_FLAGS").unwrap_or_default(),
        )
    };

    // Get the directories of all extra components to build.
    let extra_component_dirs = to_cmake_path_list(config.native.extra_component_dirs()?)?;

    // `cmake::Config` automatically uses `<out_dir>/build` and there is no way to query
    // what build directory it sets, so we hard-code it.
    let cmake_build_dir = out_dir.join("build");

    let query = cmake::Query::new(
        &cmake_build_dir,
        "cargo",
        &[ObjKind::Codemodel, ObjKind::Toolchains, ObjKind::Cache],
    )?;

    let mut cmake_config = cmake::Config::new(&out_dir);
    cmake_config
        .generator(cmake_generator.name())
        .out_dir(&out_dir)
        .no_build_target(true)
        .define("CMAKE_TOOLCHAIN_FILE", &cmake_toolchain_file)
        .define("CMAKE_BUILD_TYPE", "")
        .always_configure(true)
        .pic(false)
        .asmflag(asm_flags)
        .cflag(c_flags)
        .cxxflag(cxx_flags)
        .env("EXTRA_COMPONENT_DIRS", extra_component_dirs)
        .env("IDF_PATH", &idf.repository.worktree())
        .env("PATH", &idf.exported_path)
        .env("SDKCONFIG_DEFAULTS", defaults_files)
        .env("IDF_TARGET", &chip_name);

    if let Some(install_dir) = install_dir.path() {
        cmake_config.env("IDF_TOOLS_PATH", install_dir);
    }

    // specify the components that should be built
    if let Some(components) = &config.native.esp_idf_components {
        cmake_config.env("COMPONENTS", components.join(";"));
    }

    // Build the esp-idf.
    cmake_config.build();

    let replies = query.get_replies()?;
    let target = replies
        .get_codemodel()?
        .into_first_conf()
        .get_target("libespidf.elf")
        .unwrap_or_else(|| {
            bail!("Could not read build information from cmake: Target 'libespidf.elf' not found")
        })?;

    let compiler = replies
        .get_toolchains()
        .and_then(|mut t| {
            t.take(Language::C)
                .ok_or_else(|| Error::msg("No C toolchain"))
        })
        .and_then(|t| {
            t.compiler
                .path
                .ok_or_else(|| Error::msg("No compiler path set"))
        })
        .context("Could not determine the compiler from cmake")?;

    let build_info = espidf::EspIdfBuildInfo {
        esp_idf_dir: idf.repository.worktree().to_owned(),
        exported_path_var: idf.exported_path.try_to_str()?.to_owned(),
        venv_python: idf.venv_python,
        build_dir: cmake_build_dir.clone(),
        project_dir: out_dir.clone(),
        compiler: compiler.clone(),
        mcu: chip_name,
        sdkconfig,
        sdkconfig_defaults: Some(sdkconfig_defaults),
    };

    // Save information about the esp-idf build to the out dir so that it can be
    // easily retrieved by tools that need it.
    build_info.save_json(out_dir.join(espidf::BUILD_INFO_FILENAME))?;

    // Get all component names built by the esp-idf (cached by `CMakeLists.txt`).
    let components = replies
        .get_cache()?
        .entries
        .iter()
        .find(|e| e.name == "BUILD_COMPONENTS")
        .ok_or_else(|| Error::msg("could not get built esp-idf components from cmake"))?
        .value
        .split(';')
        .filter_map(|comp| match comp.trim() {
            c if c.is_empty() => None,
            c => Some(c.to_string()),
        })
        .collect::<Vec<_>>();

    eprintln!("Built components: {}", components.join(", "));

    let sdkconfig_json = path_buf![&cmake_build_dir, "config", "sdkconfig.json"];
    let build_output = EspIdfBuildOutput {
        cincl_args: build::CInclArgs::try_from(&target.compile_groups[0])?,
        link_args: Some(
            build::LinkArgsBuilder::try_from(&target.link.unwrap())?
                .linker(&compiler)
                .working_directory(&cmake_build_dir)
                .force_ldproxy(true)
                .build()?,
        ),
        bindgen: bindgen::Factory::from_cmake(&target.compile_groups[0])?.with_linker(&compiler),
        components: EspIdfComponents::from(components),
        kconfig_args: Box::new(
            kconfig::try_from_json_file(sdkconfig_json.clone())
                .with_context(|| anyhow!("Failed to read '{:?}'", sdkconfig_json))?,
        ),
        env_path: Some(idf.exported_path.try_to_str()?.to_owned()),
        esp_idf: build_info.esp_idf_dir,
        config,
    };

    Ok(build_output)
}

// Generate `sdkconfig.defaults` content based on the crate manifest (`Cargo.toml`).
//
// This is currently only used to forward the optimization options to the esp-idf.
fn generate_sdkconfig_defaults() -> Result<String> {
    const OPT_VARS: [&str; 4] = [
        "CONFIG_COMPILER_OPTIMIZATION_NONE",
        "CONFIG_COMPILER_OPTIMIZATION_DEFAULT",
        "CONFIG_COMPILER_OPTIMIZATION_PERF",
        "CONFIG_COMPILER_OPTIMIZATION_SIZE",
    ];

    let opt_level = env::var("OPT_LEVEL")?;
    let debug = env::var("DEBUG")?;
    let opt_index = match (opt_level.as_str(), debug.as_str()) {
        ("s" | "z", _) => 3,               // -Os
        ("1", _) | (_, "2" | "true") => 1, // -Og
        ("0", _) => 0,                     // -O0
        ("2" | "3", _) => 2,               // -O2
        _ => unreachable!("Invalid DEBUG or OPT_LEVEL"),
    };

    Ok(OPT_VARS
        .iter()
        .enumerate()
        .map(|(i, s)| format!("{}={}\n", s, if i == opt_index { 'y' } else { 'n' }))
        .collect::<String>())
}
