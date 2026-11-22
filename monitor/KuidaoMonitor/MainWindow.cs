using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Layout;
using Avalonia.Media;
using Avalonia.Platform;
using Avalonia.Threading;

namespace KuidaoMonitor;

public sealed class MainWindow : Window
{
  private enum View { Monitor = 0, Network = 1, Release = 2, SelfCheck = 3, Suggestions = 4 }

  private static readonly string[] ViewNames = { "監控", "網路", "發布", "自檢", "建議" };

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
  private readonly Button _tokenButton = Ui.SmallButton("設定 Token", Ui.Blue);
  private readonly Button _releaseTokenButton = Ui.SmallButton("設定 Token", Ui.Blue);
  private readonly TextBlock _systemNet = Ui.Text("0.0 kbps", 18, Ui.Blue, true);
  private readonly TextBlock _kuidaoNet = Ui.Text("0.0 kbps", 18, Ui.Green, true);
  private readonly TextBlock _updated = Ui.Text("尚未更新");
  private readonly StackPanel _commandPanel = new() { Spacing = 7 };
  private readonly StackPanel _processPanel = new() { Spacing = 7 };
  private readonly StackPanel _connectionPanel = new() { Spacing = 7 };
  private readonly StackPanel _checkPanel = new() { Spacing = 9 };
  private readonly StackPanel _downloadPanel = new() { Spacing = 9 };
  private readonly StackPanel _suggestionPanel = new() { Spacing = 8 };
  private readonly StackPanel _releasePanel = new() { Spacing = 8 };
  private readonly Panel _viewHost = new();
  private readonly Button[] _navButtons = new Button[5];
  private readonly DualSparkline _spark = new() { Height = 130 };

  public MainWindow()
  {
    _selfCheck = new SelfCheckService(_github);
    Title = "鬼島監控台";
    Width = 1440; Height = 900; MinWidth = 1200; MinHeight = 760;
    Background = Ui.Bg;
    var icon = LoadIcon();
    if (icon != null) Icon = icon;
    Content = Build();
    var local = new DispatcherTimer { Interval = TimeSpan.FromSeconds(2) };
    local.Tick += (_, _) => PollLocal();
    local.Start();
    var remote = new DispatcherTimer { Interval = TimeSpan.FromSeconds(60) };
    remote.Tick += async (_, _) => await RefreshGitHubAsync();
    DispatcherTimer.RunOnce(async () => await RefreshAllAsync(), TimeSpan.FromMilliseconds(800));
    remote.Start();
  }

  private static WindowIcon? LoadIcon()
  {
    try
    {
      var uri = new Uri("avares://KuidaoMonitor/assets/monitor.ico");
      using var stream = AssetLoader.Open(uri);
      return new WindowIcon(stream);
    }
    catch
    {
      return null;
    }
  }

  private Control Build()
  {
    var root = new Grid();
    root.ColumnDefinitions.Add(new ColumnDefinition(GridLength.Auto));
    root.ColumnDefinitions.Add(new ColumnDefinition(GridLength.Star));

    var rail = new StackPanel
    {
      Width = 76,
      Background = Brush.Parse("#0F1218"),
      Spacing = 10,
      HorizontalAlignment = HorizontalAlignment.Center,
      Margin = new Thickness(0, 22, 0, 0)
    };
    rail.Children.Add(new Border
    {
      Width = 42, Height = 42, CornerRadius = new CornerRadius(13), Background = Ui.Blue,
      Margin = new Thickness(0, 0, 0, 10),
      Child = Ui.Text("鬼", 18, Brushes.White, true, HorizontalAlignment.Center, VerticalAlignment.Center)
    });
    for (int i = 0; i < ViewNames.Length; i++)
    {
      var index = i;
      var btn = Ui.NavButton(ViewNames[i]);
      btn.Click += (_, _) => ShowView((View)index);
      _navButtons[i] = btn;
      rail.Children.Add(btn);
    }
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
    var refresh = Ui.SmallButton("立即自檢", Ui.Blue);
    refresh.Click += async (_, _) => await RefreshAllAsync();
    actions.Children.Add(refresh); actions.Children.Add(_updated);
    Grid.SetColumn(actions, 1); top.Children.Add(actions);
    main.Children.Add(top);

    BuildViews();
    main.Children.Add(_viewHost);

    var scroll = new ScrollViewer
    {
      Content = main,
      HorizontalScrollBarVisibility = ScrollBarVisibility.Disabled,
      VerticalScrollBarVisibility = ScrollBarVisibility.Auto
    };
    Grid.SetColumn(scroll, 1); root.Children.Add(scroll);
    ShowView(View.Monitor);
    return root;
  }

  private void BuildViews()
  {
    // 監控
    var monitor = new StackPanel { Spacing = 14 };
    var stats = new Grid { ColumnDefinitions = new ColumnDefinitions("*,*,*,*") };
    AddStat(stats, 0, "執行程序", _statProcesses, "本機鬼島相關 PID");
    AddStat(stats, 1, "指令事件", _statCommands, "啟動／結束記錄");
    AddStat(stats, 2, "下載總數", _statDownloads, "Release 資產累計");
    var trafficCard = BuildTrafficStat();
    Grid.SetColumn(trafficCard, 3); stats.Children.Add(trafficCard);
    monitor.Children.Add(stats);

    var overviewGrid = new Grid { ColumnDefinitions = new ColumnDefinitions("1.25*,1*") };
    var command = Ui.Card("鬼島執行指令", out var cmdBody, "即時");
    cmdBody.Children.Add(_commandPanel);
    Grid.SetColumn(command, 0);
    var procs = Ui.Card("本機鬼島程序", out var procBody, "PID");
    procBody.Children.Add(_processPanel);
    Grid.SetColumn(procs, 1);
    var overviewRight = new StackPanel { Spacing = 14, Margin = new Thickness(14, 0, 0, 0) };
    overviewRight.Children.Add(procs);
    Grid.SetColumn(overviewRight, 1);
    overviewGrid.Children.Add(command); overviewGrid.Children.Add(overviewRight);
    monitor.Children.Add(overviewGrid);

    // 網路
    var network = new StackPanel { Spacing = 14 };
    var net = Ui.Card("網路流量", out var netBody, "即時");
    var netStats = new Grid { ColumnDefinitions = new ColumnDefinitions("*,*") };
    netStats.Children.Add(Labeled("全系統", _systemNet));
    var labeled2 = Labeled("鬼島程序", _kuidaoNet);
    Grid.SetColumn(labeled2, 1); netStats.Children.Add(labeled2);
    netBody.Children.Add(netStats); netBody.Children.Add(_spark); netBody.Children.Add(_connectionPanel);
    network.Children.Add(net);

    // 發布
    var releaseView = new StackPanel { Spacing = 14 };
    var releaseGrid = new Grid { ColumnDefinitions = new ColumnDefinitions("1.25*,1*") };
    var release = Ui.Card("網站與發布", out var releaseBody, "GitHub");
    releaseBody.Children.Add(_releasePanel);
    var releaseTokenRow = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10, Margin = new Thickness(0, 4, 0, 0) };
    releaseTokenRow.Children.Add(_releaseTokenButton);
    releaseTokenRow.Children.Add(Ui.Text("權杖僅儲存於本機", 9.5));
    releaseBody.Children.Add(releaseTokenRow);
    Grid.SetColumn(release, 0);
    var downloads = Ui.Card("下載次數", out var dlBody, "資產");
    dlBody.Children.Add(_downloadPanel);
    Grid.SetColumn(downloads, 1);
    var releaseRight = new StackPanel { Margin = new Thickness(14, 0, 0, 0) };
    releaseRight.Children.Add(downloads);
    Grid.SetColumn(releaseRight, 1);
    releaseGrid.Children.Add(release); releaseGrid.Children.Add(releaseRight);
    releaseView.Children.Add(releaseGrid);

    // 自檢
    var selfView = new StackPanel { Spacing = 14 };
    var check = Ui.Card("自檢模式", out var checkBody, "進度");
    checkBody.Children.Add(_checkPanel);
    selfView.Children.Add(check);

    // 建議
    var suggestView = new StackPanel { Spacing = 14 };
    var suggest = Ui.Card("建議新增與修復", out var sugBody, "優先");
    sugBody.Children.Add(_suggestionPanel);
    suggestView.Children.Add(suggest);

    _viewHost.Children.Add(monitor);
    _viewHost.Children.Add(network);
    _viewHost.Children.Add(releaseView);
    _viewHost.Children.Add(selfView);
    _viewHost.Children.Add(suggestView);

    _tokenButton.Click += TokenButtonClick;
    _releaseTokenButton.Click += TokenButtonClick;
  }

  private Border BuildTrafficStat()
  {
    var panel = new StackPanel { Spacing = 7 };
    panel.Children.Add(Ui.Text("倉庫流量", 10.5, Ui.Muted));
    panel.Children.Add(_statRepo);
    var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
    row.Children.Add(_tokenButton);
    panel.Children.Add(row);
    return new Border
    {
      Background = Ui.CardBg, BorderBrush = Ui.Line, BorderThickness = new Thickness(1),
      CornerRadius = new CornerRadius(16), Padding = new Thickness(16), Child = panel
    };
  }

  private void ShowView(View view)
  {
    for (int i = 0; i < _viewHost.Children.Count; i++)
      _viewHost.Children[i].IsVisible = i == (int)view;
    for (int i = 0; i < _navButtons.Length; i++)
      Ui.SetNavActive(_navButtons[i], i == (int)view);
  }

  private static void AddStat(Grid g, int col, string label, TextBlock value, string sub)
  {
    var card = Ui.StatCard(label, value, sub, Ui.Blue);
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
    Fill(_processPanel, _processes.Select(p => ProcessRow(p)));
    Fill(_connectionPanel, _connections.Take(6).Select(c => Row(c.Local, c.Remote, c.State, $"IN {c.BytesIn} / OUT {c.BytesOut}")));
  }

  private static Grid ProcessRow(ProcessSnapshot p)
  {
    var g = new Grid { ColumnDefinitions = new ColumnDefinitions("56,*,64,72") };
    var cols = new[] { p.Pid.ToString(), p.Name, $"{p.CpuPercent:0.0}%", $"{p.MemoryMb:0.0} MB" };
    for (int i = 0; i < cols.Length; i++)
    {
      var t = Ui.Text(cols[i], 10, i == 0 ? Ui.Blue : Ui.Muted);
      Grid.SetColumn(t, i); g.Children.Add(t);
    }
    return g;
  }

  private void UpdateGitHubPanels()
  {
    if (_githubData is null) return;
    _statDownloads.Text = _githubData.TotalDownloads.ToString();
    var authorized = _githubData.Traffic.Authorized;
    _statRepo.Text = authorized ? $"{_githubData.Traffic.Views}/{_githubData.Traffic.Clones}" : "需 Token";
    var label = _github.HasToken ? "更新 Token" : "設定 Token";
    SetButtonLabel(_tokenButton, label);
    SetButtonLabel(_releaseTokenButton, label);
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

  private static void SetButtonLabel(Button btn, string label)
  {
    if (btn.Content is TextBlock t)
      t.Text = label;
    else
      btn.Content = Ui.Text(label, 10.5, Brushes.White, true, HorizontalAlignment.Center, VerticalAlignment.Center);
  }

  private async void TokenButtonClick(object? sender, EventArgs e)
  {
    string? result;
    try
    {
      result = await TokenDialog.ShowAsync(this, _github.HasToken ? "" : null);
    }
    catch { return; }
    if (result is null) return;
    _github.SetToken(result.Length == 0 ? null : result);
    await RefreshGitHubAsync();
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
    var any = false;
    foreach (var r in rows) { panel.Children.Add(r); any = true; }
    if (!any) panel.Children.Add(Ui.Text("等待資料…", 10, Ui.Muted));
  }
}
