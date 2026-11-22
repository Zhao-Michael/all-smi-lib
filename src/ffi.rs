//! C ABI facade for embedding all-smi in a shared library
//! (`all_smi.dll` / `liball_smi.so` / `liball_smi.dylib`).
//!
//! The module is deliberately tiny and depends only on the core library
//! surface (`client`, `device`, `storage`, `error`), so it compiles with
//! `--no-default-features` (no CLI/TUI/HTTP dependencies).
//!
//! # Calling convention
//!
//! * All functions are `extern "C"`; call them with the C calling convention.
//! * **Handle model:** create a handle with [`allsmi_new`] /
//!   [`allsmi_new_with_interval`], destroy it with [`allsmi_free`]. Hardware
//!   detection happens once at handle creation; the data functions are cheap
//!   per-call refreshes. The handle is safe to share across threads
//!   (`AllSmi` is `Send + Sync`).
//! * Data functions return **UTF-8 JSON strings** allocated by the library;
//!   release them with [`allsmi_free_string`]. On failure the return value is
//!   `NULL` — query [`allsmi_last_error`] (thread-local) for the message.
//! * Every function is panic-safe: a Rust panic is converted into `NULL`
//!   plus an error message, never into an unwind across the FFI boundary.
//!
//! # JSON schemas
//!
//! * `allsmi_cpu_json`     → JSON array of `CpuInfo`
//! * `allsmi_memory_json`  → JSON array of `MemoryInfo`
//! * `allsmi_gpu_json`     → JSON array of `GpuInfo`
//! * `allsmi_storage_json` → JSON array of `StorageInfo`
//! * `allsmi_chassis_json` → JSON object `ChassisInfo`, or `null`
//! * `allsmi_process_json` → JSON array of `ProcessInfo` (GPU processes)
//! * `allsmi_vgpu_json`    → JSON array of `VgpuHostInfo`
//! * `allsmi_mig_json`     → JSON array of `MigGpuInfo`
//!
//! Field names follow the serde (snake_case) representation of the Rust
//! structs in [`crate::device`] and [`crate::storage`].
//!
//! # Example (C)
//!
//! ```c,no_run
//! #include "allsmi.h"
//!
//! allsmi_handle_t* h = allsmi_new();
//! if (!h) { fprintf(stderr, "init: %s\n", allsmi_last_error()); return 1; }
//!
//! const char* cpu = allsmi_cpu_json(h);
//! if (cpu) {
//!     printf("%s\n", cpu);          // UTF-8 JSON
//!     allsmi_free_string(cpu);
//! }
//! allsmi_free(h);
//! ```

use std::cell::RefCell;
use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::sync::Arc;
use std::thread_local;

use serde::Serialize;

use crate::client::{AllSmi, AllSmiConfig};

/// Opaque handle wrapping an `Arc<AllSmi>`.
///
/// Callers never dereference it; `#[repr(C)]` with a single pointer-sized
/// field keeps the layout trivial and stable across ABI boundaries.
#[repr(C)]
pub struct AllSmiHandle {
    smi: Arc<AllSmi>,
}

thread_local! {
    /// Thread-local error message set by the most recent failed call.
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn set_last_error(msg: String) {
    LAST_ERROR.with(|slot| {
        // `CString::new` panics on interior NULs; replace them so this
        // helper itself can never panic.
        let c = CString::new(msg.replace('\0', " ")).expect("interior NULs replaced");
        *slot.borrow_mut() = Some(c);
    });
}

/// Core helper: run `f` on the handle, serialize the result to JSON, and
/// translate every failure mode (null handle, operation error,
/// serialization error, panic) into `NULL` plus a thread-local error.
fn json_call<T, F>(handle: *const AllSmiHandle, op: &str, f: F) -> *const c_char
where
    T: Serialize,
    F: FnOnce(&AllSmi) -> Result<T, String>,
{
    if handle.is_null() {
        set_last_error(format!("null handle passed to {op}"));
        return ptr::null();
    }

    let smi = unsafe { &(*handle).smi };
    let outcome = catch_unwind(AssertUnwindSafe(move || {
        let data = f(smi)?;
        let json = serde_json::to_string(&data)
            .map_err(|e| format!("failed to serialize {op} result: {e}"))?;
        CString::new(json).map_err(|e| format!("serialized JSON has interior NUL: {e}"))
    }));

    match outcome {
        Ok(Ok(cs)) => CString::into_raw(cs) as *const c_char,
        Ok(Err(e)) => {
            set_last_error(format!("{op}: {e}"));
            ptr::null()
        }
        Err(_) => {
            set_last_error(format!("{op}: Rust panic caught at FFI boundary"));
            ptr::null()
        }
    }
}

/// Create a handle, converting both `Result::Err` and panics into `NULL`
/// plus a thread-local error.
fn create_handle(op: &str, f: impl FnOnce() -> crate::Result<AllSmi>) -> *mut AllSmiHandle {
    let outcome = catch_unwind(AssertUnwindSafe(f));
    match outcome {
        Ok(Ok(smi)) => Box::into_raw(Box::new(AllSmiHandle {
            smi: Arc::new(smi),
        })),
        Ok(Err(e)) => {
            set_last_error(format!("{op}: {e}"));
            ptr::null_mut()
        }
        Err(_) => {
            set_last_error(format!("{op}: Rust panic caught at FFI boundary"));
            ptr::null_mut()
        }
    }
}

/// Initialize a new all-smi handle with default configuration.
///
/// Hardware detection (GPU drivers, CPU topology, etc.) happens here;
/// subsequent data calls are lightweight refreshes. Returns `NULL` on
/// failure — see [`allsmi_last_error`].
#[unsafe(no_mangle)]
pub extern "C" fn allsmi_new() -> *mut AllSmiHandle {
    create_handle("allsmi_new", AllSmi::new)
}

/// Initialize a new handle with a custom reader sample interval.
///
/// `interval_ms` is the per-reader refresh interval in milliseconds.
/// Returns `NULL` on failure — see [`allsmi_last_error`].
#[unsafe(no_mangle)]
pub extern "C" fn allsmi_new_with_interval(interval_ms: u32) -> *mut AllSmiHandle {
    create_handle(
        "allsmi_new_with_interval",
        move || AllSmi::with_config(AllSmiConfig::new().sample_interval(u64::from(interval_ms))),
    )
}

/// Release a handle created by [`allsmi_new`] / [`allsmi_new_with_interval`].
///
/// Safe to call with `NULL`.
#[unsafe(no_mangle)]
pub extern "C" fn allsmi_free(handle: *mut AllSmiHandle) {
    if !handle.is_null() {
        unsafe { drop(Box::from_raw(handle)) };
    }
}

/// CPU information as a JSON array of `CpuInfo`.
#[unsafe(no_mangle)]
pub extern "C" fn allsmi_cpu_json(handle: *mut AllSmiHandle) -> *const c_char {
    json_call(handle, "get_cpu_info", |smi| Ok(smi.get_cpu_info()))
}

/// Memory information as a JSON array of `MemoryInfo`.
#[unsafe(no_mangle)]
pub extern "C" fn allsmi_memory_json(handle: *mut AllSmiHandle) -> *const c_char {
    json_call(handle, "get_memory_info", |smi| Ok(smi.get_memory_info()))
}

/// GPU/NPU information as a JSON array of `GpuInfo`.
#[unsafe(no_mangle)]
pub extern "C" fn allsmi_gpu_json(handle: *mut AllSmiHandle) -> *const c_char {
    json_call(handle, "get_gpu_info", |smi| Ok(smi.get_gpu_info()))
}

/// Storage information as a JSON array of `StorageInfo`.
#[unsafe(no_mangle)]
pub extern "C" fn allsmi_storage_json(handle: *mut AllSmiHandle) -> *const c_char {
    json_call(handle, "get_storage_info", |smi| Ok(smi.get_storage_info()))
}

/// Chassis (power/thermal) information as a JSON `ChassisInfo` object, or
/// `null` when the platform has no chassis data.
#[unsafe(no_mangle)]
pub extern "C" fn allsmi_chassis_json(handle: *mut AllSmiHandle) -> *const c_char {
    json_call(handle, "get_chassis_info", |smi| Ok(smi.get_chassis_info()))
}

/// GPU process information as a JSON array of `ProcessInfo`.
#[unsafe(no_mangle)]
pub extern "C" fn allsmi_process_json(handle: *mut AllSmiHandle) -> *const c_char {
    json_call(handle, "get_process_info", |smi| Ok(smi.get_process_info()))
}

/// vGPU host/instance information as a JSON array of `VgpuHostInfo`
/// (empty array when no vGPU hardware is present).
#[unsafe(no_mangle)]
pub extern "C" fn allsmi_vgpu_json(handle: *mut AllSmiHandle) -> *const c_char {
    json_call(handle, "get_vgpu_info", |smi| Ok(smi.get_vgpu_info()))
}

/// MIG host/instance information as a JSON array of `MigGpuInfo`
/// (empty array when no MIG hardware is present).
#[unsafe(no_mangle)]
pub extern "C" fn allsmi_mig_json(handle: *mut AllSmiHandle) -> *const c_char {
    json_call(handle, "get_mig_info", |smi| Ok(smi.get_mig_info()))
}

/// Thread-local error message from the most recent failed call on this
/// thread; `NULL` when there is no error.
///
/// The pointer is owned by the library and remains valid until the next
/// failed call on the same thread (or thread exit). Copy the string if it
/// must outlive that window.
#[unsafe(no_mangle)]
pub extern "C" fn allsmi_last_error() -> *const c_char {
    LAST_ERROR.with(|slot| slot.borrow().as_ref().map_or(ptr::null(), |s| s.as_ptr()))
}

/// Release a JSON string returned by an `allsmi_*_json` function.
///
/// Safe to call with `NULL`.
#[unsafe(no_mangle)]
pub extern "C" fn allsmi_free_string(s: *const c_char) {
    if !s.is_null() {
        unsafe { drop(CString::from_raw(s as *mut c_char)) };
    }
}

/// Library version, `NUL`-terminated and valid for the process lifetime.
#[unsafe(no_mangle)]
pub extern "C" fn allsmi_version() -> *const c_char {
    const VERSION: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();
    CStr::from_bytes_with_nul(VERSION).expect("version string ends with NUL").as_ptr()
}
