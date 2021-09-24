#![cfg_attr(not(feature = "std"), no_std)]

pub mod error;
pub mod mutex;

mod alloc;
mod panic;
mod pthread_rwlock;

// ESP-IDF current stable version (4.3) has atomics for ESP32S2, but not for ESP32C3
// The ESP-IDF master branch has atomics for both
#[cfg(all(esp32c3, esp_idf_version = "4.3"))]
mod atomics_esp32c3;

pub use bindings::*;
pub use error::*;
pub use mutex::EspMutex;

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

    /// Do NOT use. This static mut is declared only as a workaround for the fact that libstd - in the link order -
    /// is *following* esp-idf-sys, which means that unless we reference outrselves the pthread_rwlock_* symbols,
    /// these will not be linked!
    pub static mut __PTHREAD_RWLOCK_INTERNAL_REFERENCE: *mut c_types::c_void =
        super::pthread_rwlock::pthread_rwlock_init as *mut _;

    #[cfg(not(doc))]
    include!(env!("EMBUILD_GENERATED_BINDINGS_FILE"));

    #[cfg(doc)]
    include!("bindings-for-docs.rs");
}
