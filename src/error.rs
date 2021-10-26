#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use core::{fmt, slice, str};

use crate::*;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum IdfError {
    Esp(EspError),
    Lwip(LwIPError),
}

impl From<EspError> for IdfError {
    fn from(err: EspError) -> Self {
        IdfError::Esp(err)
    }
}

impl From<LwIPError> for IdfError {
    fn from(err: LwIPError) -> Self {
        IdfError::Lwip(err)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for IdfError {}

impl fmt::Display for IdfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdfError::Esp(err) => write!(f, "esp-idf esp error: {}", err),
            IdfError::Lwip(err) => write!(f, "esp-idf lwip error: {}", err),
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct EspError(esp_err_t);

impl EspError {
    pub fn from(error: esp_err_t) -> Option<Self> {
        if error == 0 {
            None
        } else {
            Some(EspError(error))
        }
    }

    pub fn check_and_return<T>(error: esp_err_t, value: T) -> Result<T, Self> {
        if error == 0 {
            Ok(value)
        } else {
            Err(EspError(error))
        }
    }

    pub fn convert(error: esp_err_t) -> Result<(), Self> {
        EspError::check_and_return(error, ())
    }

    pub fn panic(&self) {
        panic!("ESP-IDF ERROR: {}", self);
    }

    pub fn code(&self) -> esp_err_t {
        self.0
    }
}

#[cfg(feature = "std")]
impl std::error::Error for EspError {}

impl fmt::Display for EspError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unsafe fn strlen(c_s: *const c_types::c_char) -> usize {
            let mut len = 0;
            while *c_s.offset(len) != 0 {
                len += 1;
            }

            len as usize
        }

        unsafe {
            let c_s = esp_err_to_name(self.code());
            str::from_utf8_unchecked(slice::from_raw_parts(c_s as *const u8, strlen(c_s))).fmt(f)
        }
    }
}

#[macro_export]
macro_rules! esp {
    ($err:expr) => {{
        esp_idf_sys::EspError::convert($err as esp_idf_sys::esp_err_t)
    }};
}

#[macro_export]
macro_rules! esp_result {
    ($err:expr, $value:expr) => {{
        esp_idf_sys::EspError::check_and_return($err as esp_idf_sys::esp_err_t, $value)
    }};
}

#[macro_export]
macro_rules! esp_nofail {
    ($err:expr) => {{
        if let Some(error) = esp_idf_sys::EspError::from($err as esp_idf_sys::esp_err_t) {
            error.panic();
        }
    }};
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct LwIPError {
    cause: Option<c_types::c_int>,
    internal: err_enum_t,
}

impl LwIPError {
    fn from_internal_error(error: c_types::c_int) -> Self {
        LwIPError {
            cause: Some(error),
            internal: unsafe { h_errno } as _,
        }
    }

    pub fn from_raw(error: err_enum_t) -> Self {
        LwIPError {
            cause: None,
            internal: error,
        }
    }

    pub fn from(error: c_types::c_int) -> Option<Self> {
        if error < 0 {
            Some(Self::from_internal_error(error))
        } else {
            None
        }
    }

    pub fn check_and_return<T>(error: c_types::c_int, value: T) -> Result<T, Self> {
        if error < 0 {
            Err(Self::from_internal_error(error))
        } else {
            Ok(value)
        }
    }

    pub fn convert(error: c_types::c_int) -> Result<(), Self> {
        Self::check_and_return(error, ())
    }
}

#[cfg(feature = "std")]
impl std::error::Error for LwIPError {}

impl fmt::Display for LwIPError {
    #[allow(non_upper_case_globals)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // there's a C function to do this, but it's only enabled with LWIP_DEBUG
        let err = match self.internal {
            err_enum_t_ERR_OK => "No error, everything OK.",
            err_enum_t_ERR_MEM => "Out of memory error.",
            err_enum_t_ERR_BUF => "Buffer error.",
            err_enum_t_ERR_TIMEOUT => "Timeout.",
            err_enum_t_ERR_RTE => "Routing problem.",
            err_enum_t_ERR_INPROGRESS => "Operation in progress",
            err_enum_t_ERR_VAL => "Illegal value.",
            err_enum_t_ERR_WOULDBLOCK => "Operation would block.",
            err_enum_t_ERR_USE => "Address in use.",
            err_enum_t_ERR_ALREADY => "Already connecting.",
            err_enum_t_ERR_ISCONN => "Conn already established.",
            err_enum_t_ERR_CONN => "Not connected.",
            err_enum_t_ERR_IF => "Low-level netif error",
            err_enum_t_ERR_ABRT => "Connection aborted.",
            err_enum_t_ERR_RST => "Connection reset.",
            err_enum_t_ERR_CLSD => "Connection closed.",
            err_enum_t_ERR_ARG => "Illegal argument.",
            _ => unreachable!("all lwip errors should be covered"),
        };

        write!(f, "lwip error")?;
        if let Some(cause) = self.cause {
            write!(f, " (cause: {})", cause)?;
        }
        write!(f, ": {}", err)?;

        Ok(())
    }
}

#[macro_export]
macro_rules! lwip {
    ($err:expr) => {{
        esp_idf_sys::LwIPError::convert($err as c_int)
    }};
}

#[macro_export]
macro_rules! lwip_result {
    ($err:expr, $value:expr) => {{
        esp_idf_sys::LwIPError::check_and_return($err as c_int, $value)
    }};
}

#[macro_export]
macro_rules! lwip_nofail {
    ($err:expr) => {{
        if let Some(error) = esp_idf_sys::LwIPError::from($err as c_int) {
            error.panic();
        }
    }};
}
