using System.Net;

namespace KuidaoMonitor;

public sealed class SelfCheckService
{
  private readonly GitHubService _github;
  public List<CheckItem> Checks { get; private set; } = new();
  public List<Suggestion> Suggestions { get; private set; } = new();

  public SelfCheckService(GitHubService github) => _github = github;

  public async Task RunAsync(IReadOnlyList<ProcessSnapshot> processes, NetworkService network, CancellationToken ct = default)
  {
    Checks = new List<CheckItem>
    {
      new("GitHub API 連線", "執行中", 10, "等待回應"),
      new("GitHub Pages 部署", "等待", 5, "等待 API"),
      new("CI 建置狀態", "等待", 5, "等待 API"),
      new("Release 安裝檔", "等待", 5, "等待 HEAD"),
      new("鬼島本機程序", "等待", 5, "等待程序快照"),
      new("本機資料夾寫入", "等待", 5, "等待測試")
    };

    GitHubData? data = null;
    try
    {
      data = await _github.GetAsync(ct);
      Checks[0] = Checks[0] with { Status = "正常", Progress = 100, Detail = $"Rate {data.RateLimitRemaining}/{data.RateLimit}" };
    }
    catch (Exception ex)
    {
      Checks[0] = Checks[0] with { Status = "異常", Progress = 100, Detail = ex.Message };
    }

    if (data is not null)
    {
      Checks[1] = new CheckItem("GitHub Pages 部署", data.PagesStatus == "built" ? "正常" : "注意", data.PagesStatus == "built" ? 100 : 70, data.PagesStatus);
      Checks[2] = new CheckItem("CI 建置狀態", data.LatestCi.Contains("success", StringComparison.OrdinalIgnoreCase) ? "正常" : "注意", data.SuccessRuns * 100 / Math.Max(1, data.TotalRuns), $"{data.SuccessRuns}/{data.TotalRuns} 成功");
      Checks[3] = await CheckReleaseAsync(data);
    }

    var kuidao = processes.Where(p => p.Name.Contains("kuidao", StringComparison.OrdinalIgnoreCase)).ToList();
    Checks[4] = kuidao.Count > 0
      ? new CheckItem("鬼島本機程序", "執行中", 100, $"{kuidao.Count} 個程序，記憶體 {kuidao.Sum(p => p.MemoryMb):0.0} MB")
      : new CheckItem("鬼島本機程序", "未執行", 35, "未發現程序");

    try
    {
      var dir = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "KuidaoMonitor");
      Directory.CreateDirectory(dir);
      var probe = Path.Combine(dir, "write-probe.tmp");
      await File.WriteAllTextAsync(probe, "ok", ct);
      File.Delete(probe);
      Checks[5] = new CheckItem("本機資料夾寫入", "正常", 100, dir);
    }
    catch (Exception ex) { Checks[5] = new CheckItem("本機資料夾寫入", "異常", 100, ex.Message); }

    BuildSuggestions(data, kuidao, network);
  }

  private static async Task<CheckItem> CheckReleaseAsync(GitHubData data)
  {
    try
    {
      using var http = new HttpClient { Timeout = TimeSpan.FromSeconds(15) };
      var req = new HttpRequestMessage(HttpMethod.Head, "https://github.com/JCnTEA/kuidao-tw/releases/latest/download/Kuidao-Setup-TW-x64.exe");
      using var resp = await http.SendAsync(req, HttpCompletionOption.ResponseHeadersRead);
      return new CheckItem("Release 安裝檔", resp.IsSuccessStatusCode ? "正常" : "異常", resp.IsSuccessStatusCode ? 100 : 60, $"{(int)resp.StatusCode} {resp.StatusCode}");
    }
    catch (Exception ex) { return new CheckItem("Release 安裝檔", "異常", 60, ex.Message); }
  }

  private void BuildSuggestions(GitHubData? data, IReadOnlyList<ProcessSnapshot> kuidao, NetworkService network)
  {
    Suggestions = new List<Suggestion>();
    if (data is null)
    {
      Suggestions.Add(new Suggestion("高", "GitHub 連線失敗", "檢查網路、代理或 API 限流", "https://github.com/JCnTEA/kuidao-tw"));
      return;
    }
    if (!data.LatestCi.Contains("success", StringComparison.OrdinalIgnoreCase))
      Suggestions.Add(new Suggestion("高", "最新 CI 未成功", data.LatestCi, "https://github.com/JCnTEA/kuidao-tw/actions"));
    if (data.PagesStatus != "built")
      Suggestions.Add(new Suggestion("中", "GitHub Pages 尚未完成部署", data.PagesStatus, "https://jcntea.github.io/kuidao-tw/"));
    if (!data.Traffic.Authorized)
      Suggestions.Add(new Suggestion("中", "設定 GitHub Token 以查看倉庫流量", "Traffic API 需要 repo 權限，可讀取 GH_TOKEN 或 gh 登入", "https://github.com/settings/tokens"));
    if (data.RateLimitRemaining < 10)
      Suggestions.Add(new Suggestion("中", "未認證 API 額度即將用盡", $"剩餘 {data.RateLimitRemaining}", "https://docs.github.com/rest/overview/resources-in-the-rest-api#rate-limiting"));
    if (kuidao.Count == 0)
      Suggestions.Add(new Suggestion("中", "鬼島目前未執行", "啟動 kuidao 或 kuidao-bootstrap 後監控台會自動接上", "https://jcntea.github.io/kuidao-tw/"));
    if (network.AdminRequired)
      Suggestions.Add(new Suggestion("低", "以系統管理員執行可取得程序位元組統計", "目前仍可顯示連線與全系統流量", ""));
    if (data.OpenIssues > 0)
      Suggestions.Add(new Suggestion("低", "待處理 Issues", $"{data.OpenIssues} 個開放議題", "https://github.com/JCnTEA/kuidao-tw/issues"));
    if (Suggestions.Count == 0)
      Suggestions.Add(new Suggestion("正常", "目前沒有需要立即處理的項目", "所有自檢皆通過", ""));
  }
}
