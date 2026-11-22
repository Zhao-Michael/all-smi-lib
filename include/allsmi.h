// all-smi C ABI (FFI) header.
//
// Companion header for the Rust `cdylib` build artifacts:
//   all_smi.dll (Windows) / liball_smi.so (Linux) / liball_smi.dylib (macOS)
//
// Build the library with:
//   cargo build --no-default-features --profile ffi
//
// Call with the C calling convention. All functions are panic-safe: a
// failure returns NULL (handle/string) and sets the thread-local error
// message retrievable via allsmi_last_error().

#ifndef ALLSMI_H
#define ALLSMI_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Opaque handle. Create with allsmi_new() / allsmi_new_with_interval(),
// destroy with allsmi_free(). Safe to share across threads.
typedef void allsmi_handle_t;

/// Initialize with default configuration. Hardware detection happens here;
/// subsequent data calls are lightweight refreshes.
/// Returns NULL on failure (see allsmi_last_error()).
allsmi_handle_t *allsmi_new(void);

/// Initialize with a custom reader sample interval in milliseconds.
/// Returns NULL on failure (see allsmi_last_error()).
allsmi_handle_t *allsmi_new_with_interval(uint32_t interval_ms);

/// Release a handle. Safe to call with NULL.
void allsmi_free(allsmi_handle_t *handle);

/* ------------------------------------------------------------------ */
/* Data functions: each returns a UTF-8 JSON string allocated by the  */
/* library — release it with allsmi_free_string(). On failure the     */
/* return value is NULL and allsmi_last_error() carries the message.  */
/* ------------------------------------------------------------------ */

// JSON array of CpuInfo (model, cores, utilization, per-core, frequency...).
const char *allsmi_cpu_json(const allsmi_handle_t *handle);

// JSON array of MemoryInfo (total/used/free/available, swap, utilization).
const char *allsmi_memory_json(const allsmi_handle_t *handle);

// JSON array of GpuInfo (utilization, memory, power, temperature, uuid...).
const char *allsmi_gpu_json(const allsmi_handle_t *handle);

// JSON array of StorageInfo.
const char *allsmi_storage_json(const allsmi_handle_t *handle);

// JSON ChassisInfo object (total power, thermal), or "null".
const char *allsmi_chassis_json(const allsmi_handle_t *handle);

// JSON array of ProcessInfo (GPU processes with PIDs and memory).
const char *allsmi_process_json(const allsmi_handle_t *handle);

// JSON array of VgpuHostInfo (empty array when no vGPU hardware).
const char *allsmi_vgpu_json(const allsmi_handle_t *handle);

// JSON array of MigGpuInfo (empty array when no MIG hardware).
const char *allsmi_mig_json(const allsmi_handle_t *handle);

/* ------------------------------------------------------------------ */
/* String and error helpers                                           */
/* ------------------------------------------------------------------ */

/// Thread-local error message from the most recent failed call on this
/// thread; NULL when there is no error. Owned by the library; valid until
/// the next failed call on the same thread (or thread exit) — copy the
/// string if it must outlive that window.
const char *allsmi_last_error(void);

/// Release a string returned by an allsmi_*_json() function.
/// Safe to call with NULL.
void allsmi_free_string(const char *s);

/// Library version. Valid for the lifetime of the process.
const char *allsmi_version(void);

#ifdef __cplusplus
}
#endif

#endif /* ALLSMI_H */
