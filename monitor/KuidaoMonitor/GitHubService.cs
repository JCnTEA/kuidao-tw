using System.Net.Http.Headers;
using System.Text.Json;

namespace KuidaoMonitor;

public sealed class GitHubService
{
  private const string Repo = "JCnTEA/kuidao-tw";
  private readonly HttpClient _http = new() { Timeout = TimeSpan.FromSeconds(20) };
  private string? _token;

  public bool HasToken => !string.IsNullOrWhiteSpace(_token);

  public GitHubService()
  {
    _token = NonBlank(Environment.GetEnvironmentVariable("GITHUB_TOKEN"))
      ?? NonBlank(Environment.GetEnvironmentVariable("GH_TOKEN"))
      ?? ReadSavedToken()
      ?? ReadGhToken();
    ApplyHeader();
    _http.DefaultRequestHeaders.UserAgent.ParseAdd("KuidaoMonitor");
    _http.DefaultRequestHeaders.Accept.Add(new MediaTypeWithQualityHeaderValue("application/vnd.github+json"));
  }

  /// <summary>更新執行階段權杖並寫入本機設定檔；傳入 null/空白則清除權杖。</summary>
  public void SetToken(string? token)
  {
    var value = string.IsNullOrWhiteSpace(token) ? null : token.Trim();
    try
    {
      var dir = ConfigDir();
      Directory.CreateDirectory(dir);
      var path = TokenFilePath();
      if (value is null)
      {
        if (File.Exists(path)) File.Delete(path);
      }
      else
      {
        File.WriteAllText(path, value);
      }
    }
    catch { }
    _token = value;
    ApplyHeader();
  }

  private void ApplyHeader()
  {
    if (!string.IsNullOrWhiteSpace(_token))
      _http.DefaultRequestHeaders.Authorization = new AuthenticationHeaderValue("Bearer", _token);
    else
      _http.DefaultRequestHeaders.Authorization = null;
  }

  public static string ConfigDir() =>
    Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "KuidaoMonitor");

  private static string TokenFilePath() => Path.Combine(ConfigDir(), "token.txt");

  public async Task<GitHubData> GetAsync(CancellationToken ct = default)
  {
    var repo = await GetJson($"repos/{Repo}", ct);
    var releases = await GetJson($"repos/{Repo}/releases?per_page=10", ct);
    var runs = await GetJson($"repos/{Repo}/actions/runs?per_page=10", ct);
    var pages = await TryGetJson($"repos/{Repo}/pages/builds/latest", ct);
    var views = await TryGetJson($"repos/{Repo}/traffic/views", ct);
    var clones = await TryGetJson($"repos/{Repo}/traffic/clones", ct);
    var rate = await GetJson("rate_limit", ct);

    var assets = new List<AssetDownload>();
    long total = 0;
    string latestRelease = "無";
    if (releases.RootElement.ValueKind == JsonValueKind.Array && releases.RootElement.GetArrayLength() > 0)
    {
      var first = releases.RootElement[0];
      latestRelease = first.GetProperty("tag_name").GetString() ?? "無";
      foreach (var rel in releases.RootElement.EnumerateArray())
      foreach (var asset in rel.GetProperty("assets").EnumerateArray())
      {
        var count = asset.GetProperty("download_count").GetInt64();
        total += count;
        assets.Add(new AssetDownload(
          asset.GetProperty("name").GetString() ?? "?",
          count,
          asset.GetProperty("size").GetInt64(),
          asset.GetProperty("browser_download_url").GetString() ?? ""));
      }
    }

    int success = 0, totalRuns = 0;
    string latestCi = "無";
    if (runs.RootElement.TryGetProperty("workflow_runs", out var runList))
    {
      totalRuns = runList.GetArrayLength();
      foreach (var r in runList.EnumerateArray())
      {
        if (r.GetProperty("conclusion").GetString() == "success") success++;
      }
      if (totalRuns > 0)
      {
        var first = runList[0];
        latestCi = $"{first.GetProperty("name").GetString()} · {first.GetProperty("conclusion").GetString() ?? first.GetProperty("status").GetString()}";
      }
    }

    string latestCommit = "無";
    try
    {
      var commits = await GetJson($"repos/{Repo}/commits?per_page=1", ct);
      if (commits.RootElement.ValueKind == JsonValueKind.Array && commits.RootElement.GetArrayLength() > 0)
        latestCommit = commits.RootElement[0].GetProperty("sha").GetString()?[..7] ?? "無";
    }
    catch { }

    var traffic = new RepoTraffic(
      Num(views, "count"), Num(views, "uniques"),
      Num(clones, "count"), Num(clones, "uniques"),
      views != null && views.RootElement.ValueKind == JsonValueKind.Object);

    return new GitHubData(
      Int(repo, "stargazers_count"), Int(repo, "forks_count"), Int(repo, "open_issues_count"),
      Int(repo, "subscribers_count"), latestRelease, latestCommit,
      pages?.RootElement.TryGetProperty("status", out _) == true ? pages.RootElement.GetProperty("status").GetString() ?? "unknown" : "unknown",
      latestCi, success, totalRuns, total, assets, traffic,
      rate.RootElement.GetProperty("rate").GetProperty("limit").GetInt32(),
      rate.RootElement.GetProperty("rate").GetProperty("remaining").GetInt32(),
      DateTime.Now);
  }

  private async Task<JsonDocument> GetJson(string path, CancellationToken ct)
  {
    using var resp = await _http.GetAsync("https://api.github.com/" + path, ct);
    resp.EnsureSuccessStatusCode();
    await using var stream = await resp.Content.ReadAsStreamAsync(ct);
    return await JsonDocument.ParseAsync(stream, cancellationToken: ct);
  }

  private async Task<JsonDocument?> TryGetJson(string path, CancellationToken ct)
  {
    try { return await GetJson(path, ct); }
    catch { return null; }
  }

  private static int Int(JsonDocument doc, string name) => doc.RootElement.TryGetProperty(name, out var v) && v.TryGetInt32(out var i) ? i : 0;
  private static long Num(JsonDocument? doc, string name) => doc?.RootElement.TryGetProperty(name, out var v) == true && v.ValueKind == JsonValueKind.Number ? v.GetInt64() : 0;

  private static string? NonBlank(string? value) => string.IsNullOrWhiteSpace(value) ? null : value.Trim();

  private static string? ReadSavedToken()
  {
    try
    {
      var path = TokenFilePath();
      if (!File.Exists(path)) return null;
      var text = File.ReadAllText(path).Trim();
      return string.IsNullOrWhiteSpace(text) ? null : text;
    }
    catch { return null; }
  }

  private static string? ReadGhToken()
  {
    try
    {
      var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
      var path = Path.Combine(home, ".config", "gh", "hosts.yml");
      if (!File.Exists(path)) return null;
      var match = System.Text.RegularExpressions.Regex.Match(File.ReadAllText(path), @"oauth_token:\s*(\S+)");
      return match.Success ? match.Groups[1].Value : null;
    }
    catch { return null; }
  }
}
