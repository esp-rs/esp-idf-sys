//! This module contains a macro - `esp_app_desc` - that - when invoked - defines
//! the ESP-IDF `esp_app_desc` application description with default information as per below.
//!
//! `esp_app_desc` structure fields as defined by the macro:
//! - version - from CARGO_PKG_VERSION
//! - project_name - from CARGO_PKG_NAME
//! - time - current build time
//! - date - current build date
//! - idf_ver - current IDF version from bindings
//! - app_elf_sha256 - [0; 32]
//! - min_efuse_blk_rev_full - CONFIG_ESP_EFUSE_BLOCK_REV_MIN_FULL
//! - secure_version - 0
//!
//! If you need a custom definition, don't use the macro but rather - manually define your own
//! static instance of the `esp_app_desc_t` structure in the `.rodata_desc` link section.

#[macro_export]
macro_rules! esp_app_desc {
    () => {
        // For backwards compatibility
        $crate::esp_app_desc!(false);
    };
    {} => {
        // New way to call the macro
        $crate::esp_app_desc!(true);
    };
    ($fix_date_time_swap: expr) => {
        #[no_mangle]
        #[used]
        #[link_section = ".rodata_desc"]
        #[allow(non_upper_case_globals)]
        pub static esp_app_desc: $crate::esp_app_desc_t = {
            const fn str_to_cstr_array<const C: usize>(s: &str) -> [::core::ffi::c_char; C] {
                let bytes = s.as_bytes();
                assert!(bytes.len() < C);

                let mut ret: [::core::ffi::c_char; C] = [0; C];
                let mut index = 0;
                while index < bytes.len() {
                    ret[index] = bytes[index] as _;
                    index += 1;
                }

                ret
            }

            $crate::esp_app_desc_t {
                magic_word: $crate::ESP_APP_DESC_MAGIC_WORD,
                secure_version: 0,
                reserv1: [0; 2],
                version: str_to_cstr_array(env!("CARGO_PKG_VERSION")),
                project_name: str_to_cstr_array(env!("CARGO_PKG_NAME")),
                #[cfg(not(esp_idf_app_reproducible_build))]
                time: str_to_cstr_array(if $fix_date_time_swap {
                    $crate::build_time::build_time_utc!("%H:%M:%S")
                } else {
                    $crate::build_time::build_time_utc!("%Y-%m-%d")
                }),
                #[cfg(not(esp_idf_app_reproducible_build))]
                date: str_to_cstr_array(if $fix_date_time_swap {
                    $crate::build_time::build_time_utc!("%Y-%m-%d")
                } else {
                    $crate::build_time::build_time_utc!("%H:%M:%S")
                }),
                #[cfg(esp_idf_app_reproducible_build)]
                time: [0 as ::core::ffi::c_char; 16],
                #[cfg(esp_idf_app_reproducible_build)]
                date: [0 as ::core::ffi::c_char; 16],
                idf_ver: str_to_cstr_array($crate::const_format::formatcp!(
                    "{}.{}.{}",
                    $crate::ESP_IDF_VERSION_MAJOR,
                    $crate::ESP_IDF_VERSION_MINOR,
                    $crate::ESP_IDF_VERSION_PATCH
                )),
                app_elf_sha256: [0; 32],
                #[cfg(any(
                    esp_idf_version_patch_at_least_5_1_7,
                    esp_idf_version_patch_at_least_5_2_3,
                    esp_idf_version_at_least_5_3_2,
                ))]
                min_efuse_blk_rev_full: $crate::CONFIG_ESP_EFUSE_BLOCK_REV_MIN_FULL as _,
                #[cfg(any(
                    esp_idf_version_patch_at_least_5_1_7,
                    esp_idf_version_patch_at_least_5_2_3,
                    esp_idf_version_at_least_5_3_2,
                ))]
                max_efuse_blk_rev_full: $crate::CONFIG_ESP_EFUSE_BLOCK_REV_MAX_FULL as _,
                #[cfg(esp_idf_version_at_least_5_4_0)]
                mmu_page_size: 0,
                #[cfg(esp_idf_version_at_least_5_4_0)]
                reserv3: [0; 3],
                #[cfg(esp_idf_version_at_least_5_4_0)]
                reserv2: [0; 18],
                #[cfg(any(
                    esp_idf_version_patch_at_least_5_1_7,
                    esp_idf_version_patch_at_least_5_2_3,
                    esp_idf_version_patch_at_least_5_3_2,
                ))]
                reserv2: [0; 19],
                #[cfg(not(any(
                    esp_idf_version_patch_at_least_5_1_7,
                    esp_idf_version_patch_at_least_5_2_3,
                    esp_idf_version_at_least_5_3_2,
                )))]
                reserv2: [0; 20],
            }
        };
    };
}
