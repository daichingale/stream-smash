using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.InteropServices;

sealed class ProcessUsage
{
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    struct ProcessEntry
    {
        public uint Size, Usage, ProcessId;
        public UIntPtr Heap;
        public uint ModuleId, Threads, ParentId;
        public int Priority;
        public uint Flags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 260)]
        public string Executable;
    }

    [DllImport("kernel32.dll", SetLastError = true)]
    static extern IntPtr CreateToolhelp32Snapshot(uint flags, uint processId);
    [DllImport("kernel32.dll", EntryPoint = "Process32FirstW", SetLastError = true)]
    static extern bool Process32First(IntPtr snapshot, ref ProcessEntry entry);
    [DllImport("kernel32.dll", EntryPoint = "Process32NextW", SetLastError = true)]
    static extern bool Process32Next(IntPtr snapshot, ref ProcessEntry entry);
    [DllImport("kernel32.dll")]
    static extern bool CloseHandle(IntPtr handle);

    public sealed class Reading
    {
        public double? CpuPercent;
        public long WorkingSet;
        public int ProcessCount;
    }

    readonly int root;
    readonly Stopwatch clock = Stopwatch.StartNew();
    Dictionary<int, TimeSpan> previous = new Dictionary<int, TimeSpan>();
    double lastSeconds;

    public ProcessUsage(int root) { this.root = root; }

    public Reading Sample()
    {
        double now = clock.Elapsed.TotalSeconds;
        var current = new Dictionary<int, TimeSpan>();
        var reading = new Reading();
        double cpuSeconds = 0;
        foreach (int id in Descendants(root)) {
            try {
                using (var process = Process.GetProcessById(id)) {
                    TimeSpan total = process.TotalProcessorTime;
                    long bytes = process.WorkingSet64;
                    TimeSpan before;
                    if (previous.TryGetValue(id, out before) && total >= before)
                        cpuSeconds += (total - before).TotalSeconds;
                    current.Add(id, total);
                    reading.WorkingSet += bytes;
                    reading.ProcessCount++;
                }
            } catch (ArgumentException) {
            } catch (InvalidOperationException) {
            } catch (Win32Exception) {
                // A short-lived worker can exit between enumeration and sampling.
            }
        }
        if (previous.Count > 0 && reading.ProcessCount > 0 && now > lastSeconds)
            reading.CpuPercent = Math.Min(100, cpuSeconds / ((now - lastSeconds) * Environment.ProcessorCount) * 100);
        previous = current;
        lastSeconds = now;
        return reading;
    }

    static List<int> Descendants(int root)
    {
        var ids = new List<int> { root };
        var children = new Dictionary<int, List<int>>();
        IntPtr snapshot = CreateToolhelp32Snapshot(0x00000002, 0); // TH32CS_SNAPPROCESS
        if (snapshot == new IntPtr(-1)) return ids;
        try {
            var entry = new ProcessEntry { Size = (uint)Marshal.SizeOf(typeof(ProcessEntry)) };
            if (Process32First(snapshot, ref entry)) {
                do {
                    List<int> siblings;
                    if (!children.TryGetValue((int)entry.ParentId, out siblings)) {
                        siblings = new List<int>();
                        children.Add((int)entry.ParentId, siblings);
                    }
                    siblings.Add((int)entry.ProcessId);
                } while (Process32Next(snapshot, ref entry));
            }
        } finally { CloseHandle(snapshot); }
        var seen = new HashSet<int>(ids);
        for (int i = 0; i < ids.Count; i++) {
            List<int> next;
            if (children.TryGetValue(ids[i], out next)) {
                foreach (int id in next) if (seen.Add(id)) ids.Add(id);
            }
        }
        return ids;
    }
}
