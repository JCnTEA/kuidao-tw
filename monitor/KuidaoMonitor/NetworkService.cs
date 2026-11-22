using System.Net;
using System.Net.NetworkInformation;
using System.Runtime.InteropServices;

namespace KuidaoMonitor;

public sealed class NetworkService
{
  private const int AF_INET = 2;
  private const int TCP_TABLE_OWNER_PID_ALL = 5;
  private long? _lastSystemBytes;
  private DateTime _lastAt = DateTime.Now;
  private readonly Dictionary<string, (ulong In, ulong Out)> _lastConn = new();
  private readonly Queue<TrafficPoint> _history = new();
  public bool AdminRequired { get; private set; }

  public (IReadOnlyList<ConnectionSnapshot> Connections, double SystemKbps, double KuidaoKbps, IReadOnlyList<TrafficPoint> History) Poll(IReadOnlyCollection<int> kuidaoPids)
  {
    var now = DateTime.Now;
    var elapsed = Math.Max(0.001, (now - _lastAt).TotalSeconds);
    long systemBytes = 0;
    foreach (var ni in NetworkInterface.GetAllNetworkInterfaces())
    {
      if (ni.OperationalStatus != OperationalStatus.Up || ni.NetworkInterfaceType is NetworkInterfaceType.Loopback or NetworkInterfaceType.Tunnel) continue;
      var st = ni.GetIPv4Statistics();
      systemBytes += st.BytesReceived + st.BytesSent;
    }
    double systemKbps = 0;
    if (_lastSystemBytes.HasValue)
      systemKbps = (systemBytes - _lastSystemBytes.Value) / elapsed * 8 / 1024;
    _lastSystemBytes = systemBytes;
    _lastAt = now;

    var connections = new List<ConnectionSnapshot>();
    double kuidaoKbps = 0;
    if (OperatingSystem.IsWindows())
    {
      foreach (var c in GetTcpConnections(kuidaoPids))
      {
        connections.Add(c);
        var key = $"{c.Local}->{c.Remote}";
        if (_lastConn.TryGetValue(key, out var old))
          kuidaoKbps += ((c.BytesIn + c.BytesOut) - (old.In + old.Out)) / elapsed * 8 / 1024;
        _lastConn[key] = (c.BytesIn, c.BytesOut);
      }
      foreach (var key in _lastConn.Keys.Where(k => !connections.Any(c => $"{c.Local}->{c.Remote}" == k)).ToList())
        _lastConn.Remove(key);
    }
    _history.Enqueue(new TrafficPoint(now, Math.Max(0, systemKbps), Math.Max(0, kuidaoKbps)));
    while (_history.Count > 40) _history.Dequeue();
    return (connections, Math.Max(0, systemKbps), Math.Max(0, kuidaoKbps), _history.ToArray());
  }

  private IEnumerable<ConnectionSnapshot> GetTcpConnections(IReadOnlyCollection<int> pids)
  {
    int size = 0;
    GetExtendedTcpTable(IntPtr.Zero, ref size, false, AF_INET, TCP_TABLE_OWNER_PID_ALL, 0);
    if (size <= 0) yield break;
    var buffer = Marshal.AllocHGlobal(size);
    try
    {
      var ret = GetExtendedTcpTable(buffer, ref size, false, AF_INET, TCP_TABLE_OWNER_PID_ALL, 0);
      if (ret != 0) yield break;
      var count = Marshal.ReadInt32(buffer);
      var rowSize = Marshal.SizeOf<MIB_TCPROW_OWNER_PID>();
      for (int i = 0; i < count; i++)
      {
        var ptr = IntPtr.Add(buffer, 4 + i * rowSize);
        var row = Marshal.PtrToStructure<MIB_TCPROW_OWNER_PID>(ptr);
        if (!pids.Contains((int)row.OwningPid)) continue;
        var local = $"{new IPAddress(row.LocalAddr)}:{Port(row.LocalPort)}";
        var remote = $"{new IPAddress(row.RemoteAddr)}:{Port(row.RemotePort)}";
        var stats = GetEStats(row, out var inBytes, out var outBytes);
        yield return new ConnectionSnapshot((int)row.OwningPid, StateName(row.State), local, remote, inBytes, outBytes, stats);
      }
    }
    finally { Marshal.FreeHGlobal(buffer); }
  }

  private bool GetEStats(MIB_TCPROW_OWNER_PID row, out ulong inBytes, out ulong outBytes)
  {
    inBytes = 0; outBytes = 0;
    var tcpRow = new MIB_TCPROW { State = row.State, LocalAddr = row.LocalAddr, LocalPort = row.LocalPort, RemoteAddr = row.RemoteAddr, RemotePort = row.RemotePort };
    var rw = Marshal.AllocHGlobal(1);
    var rod = Marshal.AllocHGlobal(88);
    try
    {
      Marshal.WriteByte(rw, 1);
      var set = SetPerTcpConnectionEStats(ref tcpRow, TcpConnectionEstats.Data, rw, 0, 1, 0);
      if (set == 5) AdminRequired = true;
      if (set != 0) return false;
      var get = GetPerTcpConnectionEStats(ref tcpRow, TcpConnectionEstats.Data, rw, 0, 1, IntPtr.Zero, 0, 0, rod, 0, 88);
      if (get != 0) return false;
      outBytes = (ulong)Marshal.ReadInt64(rod, 0);
      inBytes = (ulong)Marshal.ReadInt64(rod, 8);
      return true;
    }
    finally { Marshal.FreeHGlobal(rw); Marshal.FreeHGlobal(rod); }
  }

  private static int Port(uint value) => IPAddress.NetworkToHostOrder((short)(value & 0xFFFF));
  private static string StateName(uint state) => state switch
  {
    2 => "SYN_SENT", 3 => "SYN_RCVD", 4 => "ESTABLISHED", 5 => "FIN_WAIT1", 6 => "FIN_WAIT2", 7 => "CLOSE_WAIT", 8 => "CLOSING", 9 => "LAST_ACK", 10 => "CLOSE", 11 => "LISTEN", 12 => "DELETE_TCB", _ => "UNKNOWN"
  };

  [DllImport("iphlpapi.dll", SetLastError = true)]
  private static extern uint GetExtendedTcpTable(IntPtr table, ref int size, bool order, int af, int tableClass, uint reserved);
  [DllImport("iphlpapi.dll", SetLastError = true)]
  private static extern uint SetPerTcpConnectionEStats(ref MIB_TCPROW row, TcpConnectionEstats type, IntPtr rw, uint rwVersion, uint rwSize, uint reserved);
  [DllImport("iphlpapi.dll", SetLastError = true)]
  private static extern uint GetPerTcpConnectionEStats(ref MIB_TCPROW row, TcpConnectionEstats type, IntPtr rw, uint rwVersion, uint rwSize, IntPtr ros, uint rosVersion, uint rosSize, IntPtr rod, uint rodVersion, uint rodSize);

  [StructLayout(LayoutKind.Sequential)]
  private struct MIB_TCPROW { public uint State; public uint LocalAddr; public uint LocalPort; public uint RemoteAddr; public uint RemotePort; }
  [StructLayout(LayoutKind.Sequential)]
  private struct MIB_TCPROW_OWNER_PID { public uint State; public uint LocalAddr; public uint LocalPort; public uint RemoteAddr; public uint RemotePort; public uint OwningPid; }
  private enum TcpConnectionEstats { Data, Segments, Path, SendBandwidthEstimation, FineRtt, LocalAddress, Observation, MaximumReassembly, Snmp, Bandwidth, Timestamp }
}
