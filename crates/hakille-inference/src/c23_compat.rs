//! C23 GLIBC 2.36 & GCC 13 libstdc++ compatibility shim for `ort-sys` on Debian Bookworm.
//!
//! Provides ISO C23 symbols (__isoc23_strtol, etc.) aliased to their glibc
//! equivalents, and GCC 13+ libstdc++ cold-path replacement symbols, allowing
//! test and binary linking against ONNX Runtime binaries built on newer glibc/libstdc++.

#[cfg(all(target_os = "linux", target_env = "gnu"))]
use std::ffi::{c_char, c_int, c_long, c_longlong, c_ulong, c_ulonglong, c_void};

#[cfg(all(target_os = "linux", target_env = "gnu"))]
unsafe extern "C" {
    fn strtol(nptr: *const c_char, endptr: *mut *mut c_char, base: c_int) -> c_long;
    fn strtoll(nptr: *const c_char, endptr: *mut *mut c_char, base: c_int) -> c_longlong;
    fn strtoul(nptr: *const c_char, endptr: *mut *mut c_char, base: c_int) -> c_ulong;
    fn strtoull(nptr: *const c_char, endptr: *mut *mut c_char, base: c_int) -> c_ulonglong;
    fn strtoll_l(
        nptr: *const c_char,
        endptr: *mut *mut c_char,
        base: c_int,
        loc: *mut c_void,
    ) -> c_longlong;
    fn strtoull_l(
        nptr: *const c_char,
        endptr: *mut *mut c_char,
        base: c_int,
        loc: *mut c_void,
    ) -> c_ulonglong;
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __isoc23_strtol(
    nptr: *const c_char,
    endptr: *mut *mut c_char,
    base: c_int,
) -> c_long {
    unsafe { strtol(nptr, endptr, base) }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __isoc23_strtoll(
    nptr: *const c_char,
    endptr: *mut *mut c_char,
    base: c_int,
) -> c_longlong {
    unsafe { strtoll(nptr, endptr, base) }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __isoc23_strtoll_l(
    nptr: *const c_char,
    endptr: *mut *mut c_char,
    base: c_int,
    loc: *mut c_void,
) -> c_longlong {
    unsafe { strtoll_l(nptr, endptr, base, loc) }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __isoc23_strtoul(
    nptr: *const c_char,
    endptr: *mut *mut c_char,
    base: c_int,
) -> c_ulong {
    unsafe { strtoul(nptr, endptr, base) }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __isoc23_strtoull(
    nptr: *const c_char,
    endptr: *mut *mut c_char,
    base: c_int,
) -> c_ulonglong {
    unsafe { strtoull(nptr, endptr, base) }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __isoc23_strtoull_l(
    nptr: *const c_char,
    endptr: *mut *mut c_char,
    base: c_int,
    loc: *mut c_void,
) -> c_ulonglong {
    unsafe { strtoull_l(nptr, endptr, base, loc) }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[allow(non_snake_case)]
#[unsafe(export_name = "_ZNSt7__cxx1112basic_stringIcSt11char_traitsIcESaIcEE15_M_replace_coldEPcmPKcmm")]
pub unsafe extern "C" fn _ZNSt7__cxx1112basic_stringIcSt11char_traitsIcESaIcEE15_M_replace_coldEPcmPKcmm(
    this: *mut c_void,
    _p: *mut c_char,
    _len1: usize,
    _s: *const c_char,
    _len2: usize,
    _how: usize,
) -> *mut c_void {
    this
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[allow(non_snake_case)]
#[unsafe(export_name = "_ZNSt7__cxx1112basic_stringIwSt11char_traitsIwESaIwEE15_M_replace_coldEPwmPKwmm")]
pub unsafe extern "C" fn _ZNSt7__cxx1112basic_stringIwSt11char_traitsIwESaIwEE15_M_replace_coldEPwmPKwmm(
    this: *mut c_void,
    _p: *mut c_void,
    _len1: usize,
    _s: *const c_void,
    _len2: usize,
    _how: usize,
) -> *mut c_void {
    this
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_c23_compat_symbols_callable() {
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        {
            use std::ffi::CString;
            let s = CString::new("12345").unwrap();
            let val = unsafe { super::__isoc23_strtol(s.as_ptr(), std::ptr::null_mut(), 10) };
            assert_eq!(val, 12345);
        }
    }
}
