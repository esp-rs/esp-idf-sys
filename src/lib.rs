#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(
    all(not(feature = "std"), feature = "alloc_handler"),
    feature(alloc_error_handler)
)]

pub use bindings::*;
pub use error::*;
pub use mutex::EspMutex;

pub mod error;
pub mod mutex;

mod alloc;
mod panic;
mod start;

// (Temporary code) ESP-IDF does not (yet) have a pthread rwlock implementation, which is required by STD
// We provide a quick and very hacky implementation here
#[cfg(all(feature = "std", esp_idf_version = "4.3"))]
mod pthread_rwlock;

// (Temporary code) ESP-IDF current stable version (4.3) has atomics for ESP32S2, but not for ESP32C3
// The ESP-IDF master branch has atomics for both
#[cfg(all(esp32c3, esp_idf_version = "4.3"))]
mod atomics_esp32c3;

/// A hack to make sure that the rwlock implementation and the esp32c3 atomics are linked to the final executable
/// Call this function once e.g. in the beginning of your main function
///
/// This function will become no-op once ESP-IDF V4.4 is released
pub fn link_patches() -> (*mut c_types::c_void, *mut c_types::c_void) {
    #[cfg(all(feature = "std", esp_idf_version = "4.3"))]
    let rwlock = pthread_rwlock::link_patches();

    #[cfg(any(
        not(feature = "std"),
        not(all(feature = "std", esp_idf_version = "4.3"))
    ))]
    let rwlock = core::ptr::null_mut();

    #[cfg(all(esp32c3, esp_idf_version = "4.3"))]
    let atomics = atomics_esp32c3::link_patches();

    #[cfg(not(all(esp32c3, esp_idf_version = "4.3")))]
    let atomics = core::ptr::null_mut();

    (rwlock, atomics)
}

#[cfg(feature = "std")]
#[allow(non_upper_case_globals)]
#[allow(non_camel_case_types)]
pub mod c_types {
    pub type c_void = std::os::raw::c_void;
    pub type c_uchar = std::os::raw::c_uchar;
    pub type c_schar = std::os::raw::c_schar;
    pub type c_char = std::os::raw::c_char;
    pub type c_short = std::os::raw::c_short;
    pub type c_ushort = std::os::raw::c_ushort;
    pub type c_int = std::os::raw::c_int;
    pub type c_uint = std::os::raw::c_uint;
    pub type c_long = std::os::raw::c_long;
    pub type c_ulong = std::os::raw::c_ulong;
    pub type c_longlong = std::os::raw::c_longlong;
    pub type c_ulonglong = std::os::raw::c_ulonglong;
}

#[cfg(not(feature = "std"))]
#[allow(non_upper_case_globals)]
#[allow(non_camel_case_types)]
pub mod c_types {
    pub type c_void = core::ffi::c_void;
    pub type c_uchar = u8;
    pub type c_schar = i8;
    // Even though libc and STD do use a signed char type for both RiscV & Xtensa,
    // we need to switch to unsigned char for no_std + RiscV in order to match what
    // is currently hard-coded in the cty crate (used by the CStr & CString impls in no_std):
    // https://github.com/japaric/cty/blob/master/src/lib.rs#L30
    #[cfg(target_arch = "riscv32")]
    pub type c_char = u8;
    #[cfg(not(target_arch = "riscv32"))]
    pub type c_char = i8;
    pub type c_short = i16;
    pub type c_ushort = u16;
    pub type c_int = i32;
    pub type c_uint = u32;
    pub type c_long = i32;
    pub type c_ulong = u32;
    pub type c_longlong = i64;
    pub type c_ulonglong = u64;
}

#[allow(clippy::all)]
#[allow(non_upper_case_globals)]
#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
mod bindings {
    use super::c_types;
    // use crate::types::*;

    include!(env!("EMBUILD_GENERATED_BINDINGS_FILE"));
}

// pub use crate::types::raw_types as c_types;

#[allow(non_upper_case_globals)]
#[allow(non_camel_case_types)]
#[doc(hidden)]
// wrapper required for mbedlts
pub mod types {
    pub mod raw_types {
        pub use std::os::raw::*;
    }
    
    // mbedtls assumes defs
    pub type int8_t = i8;
    pub type int16_t = i16;
    pub type int32_t = i32;
    pub type int64_t = i64;
    pub type uint8_t = u8;
    pub type uint16_t = u16;
    pub type uint32_t = u32;
    pub type uint64_t = u64;
    pub type size_t = usize;
    pub type ssize_t = isize;
    pub type intptr_t = isize;
    pub type uintptr_t = usize;
    pub type ptrdiff_t = isize;
}

pub const ECDSA_MAX_LEN: usize = 121; // TODO why is this not being generated in bindings.rs???

// pub mod c_types {
//     pub use crate::types::*;
//     pub use crate::types::raw_types::*;
// }

// #[cfg(feature = "std")]

// #[cfg(threading_component = "pthread")]
// pub use self::libc::pthread_mutex_t;

// #[cfg(feature = "zlib")]
// extern crate libz_sys;
// #[cfg(feature = "zlib")]
// pub use self::libz_sys::z_stream;

// #[cfg(feature = "pkcs11")]
// const ERROR: _PKCS11_NOT_SUPPORTED_ = ();
