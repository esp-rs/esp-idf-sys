use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

// Helper function to read existing cfg file into HashMap
fn read_cfg_file(cfg_file_path: &str) -> HashMap<String, Vec<String>> {
    let mut all_cfgs: HashMap<String, Vec<String>> = HashMap::new();

    if Path::new(cfg_file_path).exists() {
        let file = fs::File::open(cfg_file_path).expect("Failed to open cfg file");
        let reader = BufReader::new(file);

        for line in reader.lines().map_while(Result::ok) {
            let cfg_line = line.trim();
            if !cfg_line.is_empty() {
                if let Some((key, value)) = cfg_line.split_once('=') {
                    // Key-value pair
                    let key = key.trim().to_string();
                    let value = value.trim().to_string();
                    all_cfgs.entry(key).or_default().push(value);
                } else {
                    // Boolean flag
                    let key = cfg_line.to_string();
                    all_cfgs.entry(key).or_default();
                }
            }
        }
    }

    all_cfgs
}

// Helper function to write cfg HashMap to file
#[cfg(any(feature = "__collect_cfg", feature = "__collect_git_tags"))]
fn write_cfg_file(cfg_file_path: &str, all_cfgs: &HashMap<String, Vec<String>>) {
    use std::io::Write;

    let mut file = fs::File::create(cfg_file_path).expect("Failed to create cfg file");

    // Sort keys for consistent output
    let mut sorted_keys: Vec<_> = all_cfgs.keys().collect();
    sorted_keys.sort();

    for key in sorted_keys {
        let values = &all_cfgs[key];
        if values.is_empty() {
            // Boolean flag
            writeln!(file, "{key}").expect("Failed to write to cfg file");
        } else {
            // Key-value pairs - sort values for consistency
            let mut sorted_values = values.clone();
            sorted_values.sort();
            for value in sorted_values {
                writeln!(file, "{key}={value}").expect("Failed to write to cfg file");
            }
        }
    }
}

// Helper function to add cfg args to the HashMap
#[cfg(any(feature = "__collect_cfg", feature = "__collect_git_tags"))]
fn add_cfg_args_to_map(
    all_cfgs: &mut HashMap<String, Vec<String>>,
    cfg_args_iter: impl Iterator<Item = String>,
) {
    for arg in cfg_args_iter {
        if let Some((key, value)) = arg.split_once('=') {
            // Key-value pair - remove quotes if present
            let key = key.trim().to_string();
            let value = value.trim().trim_matches('"').to_string();
            let values = all_cfgs.entry(key).or_default();
            if !values.contains(&value) {
                values.push(value);
            }
        } else {
            // Boolean flag
            let key = arg.trim().to_string();
            if !key.is_empty() {
                all_cfgs.entry(key).or_default();
            }
        }
    }
}

/// Collect ESP-IDF versions from git and add them to the cfg collection
#[cfg(feature = "__collect_git_tags")]
pub(super) fn collect_git_tags() {
    use std::process::Command;

    let cfg_file_path = "build/collected_cfgs.txt";
    let mut all_cfgs = read_cfg_file(cfg_file_path);

    // Fetch ESP-IDF versions from git
    println!("cargo::warning=Fetching ESP-IDF versions from git...");
    let output = Command::new("sh")
        .arg("-c")
        .arg("git ls-remote --tags https://github.com/espressif/esp-idf.git | awk '{print $2}' | sed 's#refs/tags/##' | grep -v '\\^{}' | grep -E '^v[0-9]+\\.[0-9]+(\\.[0-9]+)?$' | sed 's/^v//' | sort -V")
        .output()
        .expect("Failed to execute git command");

    if !output.status.success() {
        panic!(
            "Git command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let versions_str = String::from_utf8_lossy(&output.stdout);
    let mut version_count = 0;

    for version_line in versions_str.lines() {
        use crate::common::EspIdfVersion;

        let version_line = version_line.trim();
        if version_line.is_empty() {
            continue;
        }

        match version_line.parse::<EspIdfVersion>() {
            Ok(version) => {
                add_cfg_args_to_map(&mut all_cfgs, version.cfg_args());
                version_count += 1;
            }
            Err(e) => {
                println!("cargo::warning=Failed to parse version '{version_line}': {e}");
            }
        }
    }

    println!("cargo::warning=Processed {version_count} ESP-IDF versions");

    // Write all cfgs back to file
    write_cfg_file(cfg_file_path, &all_cfgs);
    println!("cargo::rerun-if-changed={cfg_file_path}");
}

/// Collect cfg args from the provided arguments and add them to the collection
#[cfg(feature = "__collect_cfg")]
pub(super) fn collect_cfg_args<I>(cfg_args: I)
where
    I: Iterator<Item = String>,
{
    let cfg_file_path = "build/collected_cfgs.txt";
    let mut all_cfgs = read_cfg_file(cfg_file_path);

    // Add current cfg_args to the collection
    add_cfg_args_to_map(&mut all_cfgs, cfg_args);

    // Write all cfgs back to file
    write_cfg_file(cfg_file_path, &all_cfgs);
    println!("cargo::rerun-if-changed={cfg_file_path}");
}

/// Emit rustc-check-cfg directives for all collected cfg args
pub(super) fn emit_check_cfg() {
    let cfg_file_path = "build/collected_cfgs.txt";
    let cfg_map = read_cfg_file(cfg_file_path);

    if !cfg_map.is_empty() {
        // Emit rustc-check-cfg directives
        for (key, values) in cfg_map {
            if values.is_empty() {
                // Boolean flag: cfg(key)
                println!("cargo::rustc-check-cfg=cfg({key})");
            } else {
                // Key-value pairs: cfg(key, values("value1", "value2", ...))
                let quoted_values: Vec<String> =
                    values.into_iter().map(|v| format!("\"{v}\"")).collect();
                println!(
                    "cargo::rustc-check-cfg=cfg({}, values({}))",
                    key,
                    quoted_values.join(", ")
                );
            }
        }
    }

    println!("cargo::rerun-if-changed={cfg_file_path}");
}
