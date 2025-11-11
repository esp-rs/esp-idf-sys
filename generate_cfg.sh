#!/bin/bash

# Script to collect all cfg combinations for ESP targets

set -e  # Exit on any error

echo "Collecting cfg combinations for all ESP targets..."

# Array of: MCU, target, toolchain version, sdkconfig path
declare -a combinations=(
    "esp32c2 riscv32imc-esp-espidf +nightly build/assets/sdkconfig_bt_trouble.defaults"
    "esp32c3 riscv32imc-esp-espidf +nightly build/assets/sdkconfig_bt_trouble.defaults"
    "esp32c6 riscv32imac-esp-espidf +nightly build/assets/sdkconfig_bt_trouble.defaults"
    "esp32h2 riscv32imac-esp-espidf +nightly build/assets/sdkconfig_bt_trouble.defaults"
    #"esp32p4 riscv32imafc-esp-espidf +nightly build/assets/sdkconfig_bt_trouble.defaults"
    "esp32 xtensa-esp32-espidf +esp build/assets/sdkconfig_bt_classic.defaults"
    "esp32s2 xtensa-esp32s2-espidf +esp build/assets/sdkconfig_bt_trouble.defaults"
    "esp32s3 xtensa-esp32s3-espidf +esp build/assets/sdkconfig_bt_trouble.defaults"
)

# Remove existing collected_cfgs.txt to start fresh
rm -f build/collected_cfgs.txt
first_run=true

for combo in "${combinations[@]}"; do
    # Clean esp-idf-sys to ensure fresh build for different MCU
    echo "Cleaning esp-idf-sys for fresh build..."
    cargo clean

    read -r mcu target toolchain_ver sdkconfig_path <<< "$combo"
    echo "Building for MCU=$mcu with target=$target using cargo $toolchain_ver and sdkconfig=$sdkconfig_path..."


    # Build feature string
    feature_flags="--features __collect_cfg"
    if $first_run; then
        feature_flags+=",__collect_git_tags"
        first_run=false
    fi

    # Run the build in a clean environment to avoid variable leakage
    env -i \
        HOME="$HOME" \
        PATH="$PATH" \
        CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" \
        RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
        MCU="$mcu" \
        ESP_IDF_SDKCONFIG_DEFAULTS="$sdkconfig_path" \
        cargo "$toolchain_ver" build --target "$target" $feature_flags --examples

    echo "Completed $mcu"
    echo "---"
done

echo "All cfg combinations collected in build/collected_cfgs.txt"
echo "File contents:"
cat build/collected_cfgs.txt
