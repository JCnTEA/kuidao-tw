using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Layout;
using Avalonia.Media;
using Avalonia.Threading;

namespace KuidaoMonitor;

public sealed class MainWindow : Window
{
  private readonly ProcessService _process = new();
  private readonly NetworkService _network = new();
  private readonly GitHubService _github = new();
  private readonly SelfCheckService _selfCheck;

  private IReadOnlyList<ProcessSnapshot> _processes = Array.Empty<ProcessSnapshot>();
  private IReadOnlyList<ConnectionSnapshot> _connections = Array.Empty<ConnectionSnapshot>();
  private IReadOnlyList<TrafficPoint> _traffic = Array.Empty<TrafficPoint>();
  private GitHubData? _githubData;

  private readonly TextBlock _statProcesses = Ui.Text("0", 27, Ui.TextBrush, true);
  private readonly TextBlock _statCommands = Ui.Text("0", 27, Ui.TextBrush, true);
  private readonly TextBlock _statDownloads = Ui.Text("0", 27, Ui.TextBrush, true);
  private readonly TextBlock _statRepo = Ui.Text("未授權", 27, Ui.TextBrush, true);
  private readonly TextBlock _systemNet = Ui.Text("0.0 kbps", 18, Ui.Blue, true);
  private readonly TextBlock _kuidaoNet = Ui.Text("0.0 kbps", 18, Ui.Green, true);
  private readonly TextBlock _updated = Ui.Text("尚未更新");
  private readonly StackPanel _commandPanel = new() { Spacing = 7 };
  private readonly StackPanel _connectionPanel = new() { Spacing = 7 };
  private readonly StackPanel _checkPanel = new() { Spacing = 9 };
  private readonly StackPanel _downloadPanel = new() { Spacing = 9 };
  private readonly StackPanel _suggestionPanel = new() { Spacing = 8 };
  private readonly StackPanel _releasePanel = new() { Spacing = 8 };
  private readonly DualSparkline _spark = new() { Height = 130 };

  public MainWindow()
  {
    _selfCheck = new SelfCheckService(_github);
    Title = "鬼島監控台";
    Width = 1440; Height = 900; MinWidth = 1200; MinHeight = 760;
    Background = Ui.Bg;
    Content = Build();
    var local = new DispatcherTimer { Interval = TimeSpan.FromSeconds(2) };
    local.Tick += (_, _) => PollLocal();
    local.Start();
    var remote = new DispatcherTimer { Interval = TimeSpan.FromSeconds(60) };
    remote.Tick += async (_, _) => await RefreshGitHubAsync();
    DispatcherTimer.RunOnce(async () => await RefreshAllAsync(), TimeSpan.FromMilliseconds(800));
    remote.Start();
  }

  private Control Build()
  {
    var root = new Grid();
    root.ColumnDefinitions.Add(new ColumnDefinition(GridLength.Auto));
    root.ColumnDefinitions.Add(new ColumnDefinition(GridLength.Star));

    var rail = new StackPanel { Width = 76, Background = Brush.Parse("#0F1218"), Spacing = 18, HorizontalAlignment = HorizontalAlignment.Center, Margin = new Thickness(0, 22, 0, 0) };
    rail.Children.Add(new Border { Width = 42, Height = 42, CornerRadius = new CornerRadius(13), Background = Ui.Blue, Child = Ui.Text("鬼", 18, Brushes.White, true, HorizontalAlignment.Center, VerticalAlignment.Center) });
    foreach (var s in new[] { "監控", "網路", "發布", "自檢", "建議" })
      rail.Children.Add(Ui.Text(s, 11, Ui.Muted, horizontal: HorizontalAlignment.Center));
    Grid.SetColumn(rail, 0); root.Children.Add(rail);

    var main = new StackPanel { Margin = new Thickness(24), Spacing = 16 };
    var top = new Grid();
    top.ColumnDefinitions.Add(new ColumnDefinition(GridLength.Star));
    top.ColumnDefinitions.Add(new ColumnDefinition(GridLength.Auto));
    var title = new StackPanel { Spacing = 3 };
    title.Children.Add(Ui.Text("鬼島 MONITOR", 24, Ui.TextBrush, true));
    title.Children.Add(Ui.Text("本機程序、網路、發布與自檢總覽", 11, Ui.Muted));
    top.Children.Add(title);
    var actions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10 };
    var refresh = new Button { Content = "立即自檢", Padding = new Thickness(14, 8), Background = Ui.Blue, Foreground = Brushes.White, CornerRadius = new CornerRadius(10) };
    refresh.Click += async (_, _) => await RefreshAllAsync();
    actions.Children.Add(refresh); actions.Children.Add(_updated);
    Grid.SetColumn(actions, 1); top.Children.Add(actions);
    main.Children.Add(top);

    var stats = new Grid { ColumnDefinitions = new ColumnDefinitions("*,*,*,*") };
    AddStat(stats, 0, "執行程序", _statProcesses, "本機鬼島相關 PID");
    AddStat(stats, 1, "指令事件", _statCommands, "啟動／結束記錄");
    AddStat(stats, 2, "下載總數", _statDownloads, "Release 資產累計");
    AddStat(stats, 3, "倉庫流量", _statRepo, "Views / Clones");
    main.Children.Add(stats);

    var body = new Grid { ColumnDefinitions = new ColumnDefinitions("2*,1.25*") };
    var left = new StackPanel { Spacing = 14 };
    var right = new StackPanel { Spacing = 14, Margin = new Thickness(14, 0, 0, 0) };

    var command = Ui.Card("鬼島執行指令", out var cmdBody, "即時");
    cmdBody.Children.Add(_commandPanel); left.Children.Add(command);

    var net = Ui.Card("網路流量", out var netBody, "即時");
    var netStats = new Grid { ColumnDefinitions = new ColumnDefinitions("*,*") };
    netStats.Children.Add(Labeled("全系統", _systemNet)); netStats.Children.Add(Labeled("鬼島程序", _kuidaoNet));
    Grid.SetColumn(netStats.Children[1], 1);
    netBody.Children.Add(netStats); netBody.Children.Add(_spark); netBody.Children.Add(_connectionPanel);
    left.Children.Add(net);

    var check = Ui.Card("自檢模式", out var checkBody, "進度");
    checkBody.Children.Add(_checkPanel); left.Children.Add(check);

    var release = Ui.Card("網站與發布", out var releaseBody, "GitHub");
    releaseBody.Children.Add(_releasePanel);
    right.Children.Add(release);
    var downloads = Ui.Card("下載次數", out var dlBody, "資產");
    dlBody.Children.Add(_downloadPanel); right.Children.Add(downloads);
    var suggest = Ui.Card("建議新增與修復", out var sugBody, "優先");
    sugBody.Children.Add(_suggestionPanel); right.Children.Add(suggest);

    Grid.SetColumn(left, 0); Grid.SetColumn(right, 1); body.Children.Add(left); body.Children.Add(right);
    main.Children.Add(body);
    Grid.SetColumn(main, 1); root.Children.Add(main);
    return new ScrollViewer { Content = root, HorizontalScrollBarVisibility = ScrollBarVisibility.Disabled, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
  }

  private static void AddStat(Grid g, int col, string label, TextBlock value, string sub)
  {
    var card = Ui.StatCard(label, value, sub, col == 3 ? Ui.Orange : Ui.Blue);
    Grid.SetColumn(card, col); g.Children.Add(card);
  }

  private static StackPanel Labeled(string label, TextBlock value)
  {
    var p = new StackPanel { Spacing = 3 };
    p.Children.Add(Ui.Text(label, 10)); p.Children.Add(value); return p;
  }

  private void PollLocal()
  {
    _processes = _process.Poll();
    var pids = _processes.Select(p => p.Pid).ToHashSet();
    (_connections, var sys, var kui, _traffic) = _network.Poll(pids);
    _systemNet.Text = $"{sys:0.0} kbps";
    _kuidaoNet.Text = $"{kui:0.0} kbps";
    _spark.Update(_traffic.Select(t => t.SystemKbps).ToArray(), _traffic.Select(t => t.KuidaoKbps).ToArray());
    UpdateLocalPanels();
  }

  private async Task RefreshGitHubAsync()
  {
    try
    {
      _githubData = await _github.GetAsync();
      _updated.Text = $"更新於 {DateTime.Now:HH:mm:ss}";
      UpdateGitHubPanels();
    }
    catch (Exception ex) { _updated.Text = ex.Message; }
  }

  private async Task RefreshAllAsync()
  {
    PollLocal();
    await _selfCheck.RunAsync(_processes, _network);
    await RefreshGitHubAsync();
    UpdateSelfCheckPanels();
  }

  private void UpdateLocalPanels()
  {
    _statProcesses.Text = _processes.Count.ToString();
    _statCommands.Text = _process.Events.Count.ToString();
    Fill(_commandPanel, _process.Events.Take(8).Select(e => Row(e.At.ToString("HH:mm:ss"), e.Pid.ToString(), e.Kind, e.CommandLine)));
    Fill(_connectionPanel, _connections.Take(6).Select(c => Row(c.Local, c.Remote, c.State, $"IN {c.BytesIn} / OUT {c.BytesOut}")));
  }

  private void UpdateGitHubPanels()
  {
    if (_githubData is null) return;
    _statDownloads.Text = _githubData.TotalDownloads.ToString();
    _statRepo.Text = _githubData.Traffic.Authorized ? $"{_githubData.Traffic.Views}/{_githubData.Traffic.Clones}" : "需 Token";
    UpdateReleasePanel();
    _downloadPanel.Children.Clear();
    var max = Math.Max(1, _githubData.Assets.Max(a => a.Count));
    foreach (var a in _githubData.Assets.Take(8))
    {
      var p = new StackPanel { Spacing = 4 };
      p.Children.Add(Ui.Text($"{a.Name} · {a.Count}", 10.5));
      p.Children.Add(Ui.Progress(a.Count * 100.0 / max, Ui.Green));
      _downloadPanel.Children.Add(p);
    }
  }

  private void UpdateReleasePanel()
  {
    if (_githubData is null) return;
    var d = _githubData;
    _releasePanel.Children.Clear();
    foreach (var (name, value) in new[]
    {
      ("最新 Release", d.LatestRelease),
      ("最新 Commit", d.LatestCommit),
      ("Pages 狀態", d.PagesStatus),
      ("最新 CI", d.LatestCi),
      ("Stars / Forks / Issues", $"{d.Stars} / {d.Forks} / {d.OpenIssues}"),
      ("倉庫 Views / Clones", d.Traffic.Authorized ? $"{d.Traffic.Views} / {d.Traffic.Clones}" : "需 Token")
    })
    {
      var row = new Grid { ColumnDefinitions = new ColumnDefinitions("120,*") };
      row.Children.Add(Ui.Text(name, 10));
      var val = Ui.Text(value, 10, Ui.TextBrush);
      Grid.SetColumn(val, 1); row.Children.Add(val);
      _releasePanel.Children.Add(row);
    }
  }

  private void UpdateSelfCheckPanels()
  {
    Fill(_checkPanel, _selfCheck.Checks.Select(c => CheckRow(c)));
    Fill(_suggestionPanel, _selfCheck.Suggestions.Select(s => SuggestionRow(s)));
  }

  private static Grid Row(params string[] cols)
  {
    var g = new Grid { ColumnDefinitions = new ColumnDefinitions("70,55,70,*") };
    for (int i = 0; i < cols.Length; i++)
    {
      var t = Ui.Text(cols[i], 10, i == 2 ? Ui.Blue : Ui.Muted);
      Grid.SetColumn(t, i); g.Children.Add(t);
    }
    return g;
  }

  private static Control CheckRow(CheckItem c)
  {
    var p = new StackPanel { Spacing = 4 };
    var head = new Grid { ColumnDefinitions = new ColumnDefinitions("*,80") };
    head.Children.Add(Ui.Text(c.Name, 10.5, Ui.TextBrush));
    var st = Ui.Text(c.Status, 10, c.Status == "正常" || c.Status == "執行中" ? Ui.Green : Ui.Orange);
    Grid.SetColumn(st, 1); head.Children.Add(st);
    p.Children.Add(head); p.Children.Add(Ui.Progress(c.Progress, c.Status == "異常" ? Ui.Red : Ui.Blue));
    p.Children.Add(Ui.Text(c.Detail, 9.5));
    return p;
  }

  private static Control SuggestionRow(Suggestion s)
  {
    var color = s.Severity switch { "高" => Ui.Red, "中" => Ui.Orange, "正常" => Ui.Green, _ => Ui.Blue };
    var p = new StackPanel { Spacing = 3 };
    p.Children.Add(Ui.Text($"{s.Severity} · {s.Title}", 10.5, Ui.TextBrush, true));
    p.Children.Add(Ui.Text(s.Detail, 9.5, color));
    return p;
  }

  private static void Fill(StackPanel panel, IEnumerable<Control> rows)
  {
    panel.Children.Clear();
    foreach (var r in rows) panel.Children.Add(r);
  }
}
