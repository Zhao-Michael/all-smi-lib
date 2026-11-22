using System.Runtime.InteropServices;
using System.Text.Json;

namespace AllSmi;

/// <summary>
/// P/Invoke wrapper around the all-smi shared library
/// (all_smi.dll / liball_smi.so / liball_smi.dylib).
///
/// See include/allsmi.h for the full C ABI contract:
/// data functions return UTF-8 JSON pointers; free them with
/// <see cref="allsmi_free_string"/>; on failure they return IntPtr.Zero and
/// <see cref="allsmi_last_error"/> carries the message.
///
/// Strings are returned as <see cref="IntPtr"/> and converted with
/// <see cref="Marshal.PtrToStringUTF8"/> — the direct `string` return
/// marshaling path (LPUTF8Str/Ansi) is known to be fragile with native
/// libraries and this pattern is the standard robust approach.
/// </summary>
public static class AllSmiNative
{
    // Extension-less name: the .NET native loader applies the platform
    // convention (all_smi.dll on Windows, liball_smi.so/.dylib elsewhere).
    private const string Dll = "all_smi";

    // ------------------------------------------------------------------
    // Handle lifecycle
    // ------------------------------------------------------------------

    /// <summary>Initialize with default configuration. IntPtr.Zero on failure.</summary>
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr allsmi_new();

    /// <summary>Initialize with a custom reader sample interval (ms). IntPtr.Zero on failure.</summary>
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr allsmi_new_with_interval(uint intervalMs);

    /// <summary>Release a handle. Safe with IntPtr.Zero.</summary>
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    public static extern void allsmi_free(IntPtr handle);

    // ------------------------------------------------------------------
    // Data functions (UTF-8 JSON pointers)
    // ------------------------------------------------------------------

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr allsmi_cpu_json(IntPtr handle);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr allsmi_memory_json(IntPtr handle);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr allsmi_gpu_json(IntPtr handle);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr allsmi_storage_json(IntPtr handle);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr allsmi_chassis_json(IntPtr handle);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr allsmi_process_json(IntPtr handle);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr allsmi_vgpu_json(IntPtr handle);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr allsmi_mig_json(IntPtr handle);

    // ------------------------------------------------------------------
    // String / error helpers
    // ------------------------------------------------------------------

    /// <summary>Thread-local error from the last failed call; IntPtr.Zero when none.</summary>
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr allsmi_last_error();

    /// <summary>Release a JSON string pointer returned by an allsmi_*_json function. Safe with IntPtr.Zero.</summary>
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    public static extern void allsmi_free_string(IntPtr s);

    /// <summary>Library version pointer (valid for the process lifetime).</summary>
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr allsmi_version();

    /// <summary>Convert a native UTF-8 pointer to a managed string (null for IntPtr.Zero).</summary>
    public static string? ToString(IntPtr nativeUtf8) =>
        nativeUtf8 == IntPtr.Zero ? null : Marshal.PtrToStringUTF8(nativeUtf8);

    /// <summary>Convenience: current thread-local error text, or null.</summary>
    public static string? LastError() => ToString(allsmi_last_error());

    /// <summary>Convenience: library version text.</summary>
    public static string Version() => ToString(allsmi_version()) ?? "?";
}

/// <summary>
/// High-level .NET wrapper handling handle lifecycle and native string
/// freeing. Create once, query as often as needed:
/// <code>
/// await using var smi = AllSmiClient.Create()!;
/// var cpus = smi.GetCpuInfo();     // JsonElement
/// </code>
/// </summary>
public sealed class AllSmiClient : IAsyncDisposable
{
    private readonly IntPtr _handle;
    private bool _disposed;

    private AllSmiClient(IntPtr handle) => _handle = handle;

    /// <summary>
    /// Create a client. Returns null if the library failed to initialize
    /// (check <see cref="AllSmiNative.LastError"/> for the reason).
    /// </summary>
    public static AllSmiClient? Create(uint sampleIntervalMs = 0)
    {
        var handle = sampleIntervalMs == 0
            ? AllSmiNative.allsmi_new()
            : AllSmiNative.allsmi_new_with_interval(sampleIntervalMs);
        return handle == IntPtr.Zero ? null : new AllSmiClient(handle);
    }

    /// <summary>
    /// Query a data function, parse the JSON, and release the native string.
    /// Returns null on failure (see <see cref="AllSmiNative.LastError"/>).
    /// </summary>
    private JsonElement? Query(Func<IntPtr, IntPtr> call)
    {
        var ptr = call(_handle);
        if (ptr == IntPtr.Zero)
        {
            return null;
        }

        try
        {
            // Holding the RootElement keeps the underlying JsonDocument alive.
            return JsonDocument.Parse(AllSmiNative.ToString(ptr)!).RootElement;
        }
        finally
        {
            AllSmiNative.allsmi_free_string(ptr);
        }
    }

    public JsonElement? GetCpuInfo() => Query(AllSmiNative.allsmi_cpu_json);
    public JsonElement? GetMemoryInfo() => Query(AllSmiNative.allsmi_memory_json);
    public JsonElement? GetGpuInfo() => Query(AllSmiNative.allsmi_gpu_json);
    public JsonElement? GetStorageInfo() => Query(AllSmiNative.allsmi_storage_json);
    public JsonElement? GetChassisInfo() => Query(AllSmiNative.allsmi_chassis_json);
    public JsonElement? GetProcessInfo() => Query(AllSmiNative.allsmi_process_json);
    public JsonElement? GetVgpuInfo() => Query(AllSmiNative.allsmi_vgpu_json);
    public JsonElement? GetMigInfo() => Query(AllSmiNative.allsmi_mig_json);

    /// <summary>Library version string.</summary>
    public string Version => AllSmiNative.Version();

    public async ValueTask DisposeAsync()
    {
        if (!_disposed)
        {
            _disposed = true;
            AllSmiNative.allsmi_free(_handle);
            await Task.CompletedTask;
        }
    }
}
