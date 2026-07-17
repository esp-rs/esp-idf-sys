//! This module contains compile-time checks only, and will generate no code.

/// If any of the two constants below do not compile, you have not properly setup the rustc cfg flag `espidf_time64`:
/// When compiling against ESP-IDF V5.X or later, you need to define the following in your `.config/cargo.toml` file
/// (look for this file in the root of your binary crate):
/// ```
/// [build]
/// rustflags = "--cfg espidf_time64"
/// ```
///
/// When compiling against ESP-IDF V4.X, you need to remove the above flag
#[allow(deprecated)]
#[allow(unused)]
#[cfg(feature = "std")]
const ESP_IDF_TIME64_CHECK: ::std::os::espidf::raw::time_t = 0 as crate::time_t;
#[allow(unused)]
const ESP_IDF_TIME64_CHECK_LIBC: ::libc::time_t = 0 as crate::time_t;

/// If any of the two compile_error! items below trigger, you have not properly setup the rustc cfg flag `espidf_picolibc`:
/// When compiling against ESP-IDF V6.X or later (which uses picolibc by default), you need to define the following
/// in your `.cargo/config.toml` file (look for this file in the root of your binary crate):
/// ```
/// [build]
/// rustflags = "--cfg espidf_picolibc"
/// ```
///
/// When compiling against ESP-IDF V5.X or earlier (which uses newlib), you need to remove the above flag
#[cfg(all(espidf_picolibc, not(esp_idf_libc_picolibc)))]
compile_error!(
    "espidf_picolibc is set but ESP-IDF is not configured to use picolibc. Remove --cfg espidf_picolibc from your rustflags."
);
#[cfg(all(esp_idf_libc_picolibc, not(espidf_picolibc)))]
compile_error!(
    "ESP-IDF is configured to use picolibc but espidf_picolibc is not set. Add --cfg espidf_picolibc to your rustflags in .cargo/config.toml."
);
/// Verify that libc::O_APPEND matches the expected value for the configured C library.
/// If this does not compile, espidf_picolibc is set incorrectly in your rustflags.
#[allow(unused)]
const ESP_IDF_PICOLIBC_CHECK: () = {
    #[cfg(espidf_picolibc)]
    assert!(
        ::libc::O_APPEND == 1024,
        "libc::O_APPEND mismatch: espidf_picolibc is set but libc was not compiled with it"
    );
    #[cfg(not(espidf_picolibc))]
    assert!(
        ::libc::O_APPEND == 8,
        "libc::O_APPEND mismatch: espidf_picolibc is not set but libc was compiled with it"
    );
};

// Check for libc/esp-idf-sys type and constant mismatches.
#[cfg(feature = "std")]
mod libc;
