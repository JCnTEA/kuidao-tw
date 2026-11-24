using System.Diagnostics;
using System.Management;

namespace KuidaoMonitor;

public sealed class ProcessService
{
  private readonly Dictionary<int, (DateTime At, TimeSpan Cpu)> _cpu = new();
  private readonly HashSet<int> _known = new();
  public List<CommandEvent> Events { get; } = new();

  public IReadOnlyList<ProcessSnapshot> Poll()
  {
    if (!OperatingSystem.IsWindows()) return Array.Empty<ProcessSnapshot>();
    var rows = new List<(ManagementObject Obj, int Pid, int Parent, string Name, string? Cmd, string? Path, DateTime Started)>();
    using var searcher = new ManagementObjectSearcher("SELECT ProcessId,ParentProcessId,Name,CommandLine,ExecutablePath,CreationDate FROM Win32_Process");
    foreach (ManagementObject obj in searcher.Get())
    {
      try
      {
        var pid = Convert.ToInt32(obj["ProcessId"]);
        var parent = Convert.ToInt32(obj["ParentProcessId"]);
        var name = obj["Name"]?.ToString() ?? "?";
        var cmd = obj["CommandLine"]?.ToString();
        var path = obj["ExecutablePath"]?.ToString();
        var started = obj["CreationDate"] is null ? DateTime.Now : ManagementDateTimeConverter.ToDateTime(obj["CreationDate"].ToString()!);
        rows.Add((obj, pid, parent, name, cmd, path, started));
      }
      catch { }
    }

    var pids = rows.Select(r => r.Pid).ToHashSet();
    var result = new List<ProcessSnapshot>();
    foreach (var row in rows)
    {
      var isKuidao = IsKuidao(row.Name, row.Cmd, row.Path) || (_known.Contains(row.Parent) && pids.Contains(row.Parent));
      if (!isKuidao) continue;
      double cpu = 0, mem = 0;
      try
      {
        using var p = Process.GetProcessById(row.Pid);
        mem = p.WorkingSet64 / 1024.0 / 1024.0;
        var now = DateTime.Now;
        var current = p.TotalProcessorTime;
        if (_cpu.TryGetValue(row.Pid, out var old))
        {
          var elapsed = (now - old.At).TotalSeconds;
          cpu = Math.Max(0, (current - old.Cpu).TotalSeconds / elapsed / Environment.ProcessorCount * 100.0);
        }
        _cpu[row.Pid] = (now, current);
      }
      catch { }

      if (!_known.Contains(row.Pid))
        AddEvent(new CommandEvent(DateTime.Now, row.Pid, "啟動", row.Name, row.Cmd ?? row.Path ?? "", "running"));
      result.Add(new ProcessSnapshot(row.Pid, row.Parent, row.Name, row.Cmd ?? "", row.Path ?? "", row.Started, Math.Min(100, cpu), mem, true));
    }

    foreach (var pid in _known.Where(p => !pids.Contains(p)).ToList())
    {
      AddEvent(new CommandEvent(DateTime.Now, pid, "結束", "process", "", "exited"));
      _known.Remove(pid);
      _cpu.Remove(pid);
    }
    foreach (var pid in pids.Where(IsKuidaoPid))
      _known.Add(pid);
    return result;
  }

  private bool IsKuidaoPid(int pid)
  {
    try
    {
      using var p = Process.GetProcessById(pid);
      return IsKuidao(p.ProcessName, null, p.MainModule?.FileName);
    }
    catch { return false; }
  }

  private static bool IsKuidao(string? name, string? cmd, string? path)
  {
    foreach (var s in new[] { name, cmd, path })
      if (s?.Contains("kuidao", StringComparison.OrdinalIgnoreCase) == true) return true;
    return false;
  }

  private void AddEvent(CommandEvent e)
  {
    Events.Insert(0, e);
    if (Events.Count > 100) Events.RemoveAt(Events.Count - 1);
  }
}
