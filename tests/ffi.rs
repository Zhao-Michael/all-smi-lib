//! End-to-end tests for the C ABI surface in `src/ffi.rs`.
//!
//! The functions are declared as a local `extern "C"` block (opaque handle,
//! same signatures as the exported symbols) so the tests exercise the true
//! ABI rather than Rust-internal calls.

use std::ffi::{c_char, CStr};

use all_smi::ffi::AllSmiHandle;

type Handle = *mut std::os::raw::c_void;
type H = *mut AllSmiHandle;
type S = *const c_char;

/// `#[no_mangle]` symbols in an rlib are not pulled in by the linker when
/// only referenced through a local `extern "C"` block (rust-lang/rust#29198).
/// Holding every function pointer in a `#[used]` static forces the objects
/// to link so the local declarations below resolve to the true exported
/// C names.
#[used]
static FORCE_LINK: (
    unsafe extern "C" fn() -> H,
    unsafe extern "C" fn(u32) -> H,
    unsafe extern "C" fn(H),
    unsafe extern "C" fn(H) -> S,
    unsafe extern "C" fn(H) -> S,
    unsafe extern "C" fn(H) -> S,
    unsafe extern "C" fn(H) -> S,
    unsafe extern "C" fn(H) -> S,
    unsafe extern "C" fn(H) -> S,
    unsafe extern "C" fn(H) -> S,
    unsafe extern "C" fn(H) -> S,
    unsafe extern "C" fn() -> S,
    unsafe extern "C" fn(S),
    unsafe extern "C" fn() -> S,
) = (
    all_smi::ffi::allsmi_new,
    all_smi::ffi::allsmi_new_with_interval,
    all_smi::ffi::allsmi_free,
    all_smi::ffi::allsmi_cpu_json,
    all_smi::ffi::allsmi_memory_json,
    all_smi::ffi::allsmi_gpu_json,
    all_smi::ffi::allsmi_storage_json,
    all_smi::ffi::allsmi_chassis_json,
    all_smi::ffi::allsmi_process_json,
    all_smi::ffi::allsmi_vgpu_json,
    all_smi::ffi::allsmi_mig_json,
    all_smi::ffi::allsmi_last_error,
    all_smi::ffi::allsmi_free_string,
    all_smi::ffi::allsmi_version,
);

unsafe extern "C" {
    fn allsmi_new() -> Handle;
    fn allsmi_new_with_interval(interval_ms: u32) -> Handle;
    fn allsmi_free(handle: Handle);
    fn allsmi_cpu_json(handle: Handle) -> *const c_char;
    fn allsmi_memory_json(handle: Handle) -> *const c_char;
    fn allsmi_gpu_json(handle: Handle) -> *const c_char;
    fn allsmi_storage_json(handle: Handle) -> *const c_char;
    fn allsmi_chassis_json(handle: Handle) -> *const c_char;
    fn allsmi_process_json(handle: Handle) -> *const c_char;
    fn allsmi_vgpu_json(handle: Handle) -> *const c_char;
    fn allsmi_mig_json(handle: Handle) -> *const c_char;
    fn allsmi_last_error() -> *const c_char;
    fn allsmi_free_string(s: *const c_char);
    fn allsmi_version() -> *const c_char;
}

fn to_string(ptr: *const c_char) -> String {
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .expect("FFI strings are UTF-8")
        .to_owned()
}

fn free_string(ptr: *const c_char) {
    unsafe { allsmi_free_string(ptr) };
}

/// Query a data function and assert the result is valid JSON.
fn assert_valid_json(ptr: *const c_char) {
    if ptr.is_null() {
        let err = unsafe { allsmi_last_error() };
        let msg = unsafe {
            if err.is_null() {
                CStr::from_bytes_with_nul_unchecked(b"<no error set>\0")
            } else {
                CStr::from_ptr(err)
            }
        };
        panic!("unexpected NULL; last_error = {}", msg.to_string_lossy());
    }
    let json = to_string(ptr);
    let value: serde_json::Value = serde_json::from_str(&json)
        .unwrap_or_else(|e| panic!("invalid JSON from {json:?}: {e}"));
    assert!(value.is_array() || value.is_null() || value.is_object());
    free_string(ptr);
}

#[test]
fn version_matches_cargo_pkg_version() {
    let version = to_string(unsafe { allsmi_version() });
    assert_eq!(version, env!("CARGO_PKG_VERSION"));
}

#[test]
fn handle_roundtrip_and_all_data_functions() {
    let handle = unsafe { allsmi_new() };
    assert!(
        !handle.is_null(),
        "allsmi_new failed: {}",
        unsafe { CStr::from_ptr(allsmi_last_error()).to_string_lossy() }
    );

    // Every data function must return valid JSON (arrays, or "null" for
    // chassis) regardless of which hardware is present on this machine.
    unsafe {
        assert_valid_json(allsmi_cpu_json(handle));
        assert_valid_json(allsmi_memory_json(handle));
        assert_valid_json(allsmi_gpu_json(handle));
        assert_valid_json(allsmi_storage_json(handle));
        assert_valid_json(allsmi_chassis_json(handle));
        assert_valid_json(allsmi_process_json(handle));
        assert_valid_json(allsmi_vgpu_json(handle));
        assert_valid_json(allsmi_mig_json(handle));
    }

    unsafe { allsmi_free(handle) };
}

#[test]
fn handle_with_interval_works() {
    let handle = unsafe { allsmi_new_with_interval(500) };
    assert!(
        !handle.is_null(),
        "allsmi_new_with_interval failed: {}",
        unsafe { CStr::from_ptr(allsmi_last_error()).to_string_lossy() }
    );

    unsafe {
        assert_valid_json(allsmi_cpu_json(handle));
        allsmi_free(handle);
    }
}

#[test]
fn null_handle_is_reported_not_panicked() {
    let ptr = unsafe { allsmi_cpu_json(std::ptr::null_mut()) };
    assert!(ptr.is_null());
    let err = unsafe { allsmi_last_error() };
    assert!(!err.is_null(), "null handle must set a thread-local error");
}
