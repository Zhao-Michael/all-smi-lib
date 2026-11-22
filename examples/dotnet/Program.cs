using AllSmi;
using System.Diagnostics;
using System.Text.Json;

// Sample: create a handle, query CPU / memory / GPU, print the JSON.
// Every FFI call is timed with Stopwatch so you can see the cost of
// initialization (hardware detection) vs. per-query refresh.
//
// Prerequisite: copy the built shared library next to this executable
//   Windows:  target/ffi/all_smi.dll     →  bin/Debug/net8.0/ (or add to PATH)
//   Linux:    target/ffi/liball_smi.so
//   macOS:    target/ffi/liball_smi.dylib
// and build with:  dotnet run

var init = Stopwatch.StartNew();
await using var smi = AllSmiClient.Create()
    ?? throw new InvalidOperationException($"allsmi_new failed: {AllSmiNative.LastError() ?? "unknown error"}");
init.Stop();

Console.WriteLine($"all-smi version: {smi.Version}");
Console.WriteLine($"init (hardware detection): {init.Elapsed.TotalMilliseconds:F1} ms");

static void Show(string label, Func<JsonElement?> query, int? limit = null)
{
    var sw = Stopwatch.StartNew();
    JsonElement? data;
    try
    {
        data = query();
    }
    finally
    {
        sw.Stop();
    }

    Console.WriteLine($"--- {label} ({sw.Elapsed.TotalMilliseconds:F1} ms) ---");
    if (data is null)
    {
        Console.WriteLine($"error: {AllSmiNative.LastError()}");
        return;
    }

    var element = data.Value;
    if (limit is not null && element.ValueKind == JsonValueKind.Array)
    {
        // Re-serialize only the first `limit` entries (e.g. process list).
        element = JsonSerializer.SerializeToElement(
            element.EnumerateArray().Take(limit.Value).ToArray());
    }

    //Console.WriteLine(JsonSerializer.Serialize(element, new JsonSerializerOptions { WriteIndented = true }));
}

Show("cpu", smi.GetCpuInfo);
Show("memory", smi.GetMemoryInfo);
Show("gpu", smi.GetGpuInfo);
Show("process (top 20)", smi.GetProcessInfo, 20);
Show("storage", smi.GetStorageInfo);

return 0;
