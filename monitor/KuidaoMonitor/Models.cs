namespace KuidaoMonitor;

public sealed record CommandEvent(DateTime At, int Pid, string Kind, string Name, string CommandLine, string Status);
public sealed record ProcessSnapshot(int Pid, int ParentPid, string Name, string CommandLine, string Path, DateTime Started, double CpuPercent, double MemoryMb, bool IsKuidao);
public sealed record ConnectionSnapshot(int Pid, string State, string Local, string Remote, ulong BytesIn, ulong BytesOut, bool ByteStatsAvailable);
public sealed record TrafficPoint(DateTime At, double SystemKbps, double KuidaoKbps);
public sealed record CheckItem(string Name, string Status, int Progress, string Detail);
public sealed record Suggestion(string Severity, string Title, string Detail, string Url);
public sealed record AssetDownload(string Name, long Count, long Size, string Url);
public sealed record RepoTraffic(long Views, long UniqueViews, long Clones, long UniqueClones, bool Authorized);
public sealed record GitHubData(
  int Stars, int Forks, int OpenIssues, int Subscribers,
  string LatestRelease, string LatestCommit, string PagesStatus, string LatestCi,
  int SuccessRuns, int TotalRuns, long TotalDownloads,
  IReadOnlyList<AssetDownload> Assets, RepoTraffic Traffic,
  int RateLimit, int RateLimitRemaining, DateTime UpdatedAt);
