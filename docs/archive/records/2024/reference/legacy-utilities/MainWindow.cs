using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Documents;
using Avalonia.Layout;
using Avalonia.Media;
using Avalonia.Platform;
using Avalonia.Threading;

namespace KuidaoMonitor;

public sealed class MainWindow : Window
{
  private const bool DevBuild =
#if DEV_BUILD
    true;
#else
    false;
#endif

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
  private bool _githubFailed;
  private readonly StackPanel _commandPanel = new() { Spacing = 7 };
  private readonly StackPanel _connectionPanel = new() { Spacing = 7 };
  private readonly StackPanel _checkPanel = new() { Spacing = 9 };
  private readonly StackPanel _downloadPanel = new() { Spacing = 9 };
  private readonly StackPanel _suggestionPanel = new() { Spacing = 8 };
  private readonly StackPanel _releasePanel = new() { Spacing = 8 };
  private readonly DualSparkline _spark = new() { Height = 130 };

  // 自檢排程：全自動（時間到自動跑）與手動介入（按鈕立即跑）兩條路。
  private readonly ScheduleSettings _schedule = ScheduleSettings.Load();
  private readonly CheckBox _autoToggle = new() { Content = "啟用自動排程" };
  private readonly TextBox _selfTime = new() { Width = 84 };
  private readonly TextBox _updateTime = new() { Width = 84 };
  private readonly TextBlock _scheduleStatus = Ui.Text("尚未執行");
  private readonly TextBlock _nextRun = Ui.Text("—");
  private bool _selfCheckRunning;

  // 分頁導覽
  private readonly List<Button> _navButtons = new();
  private readonly List<Control> _pages = new();
  private readonly Grid _contentHost = new();

  public MainWindow()
  {
    _selfCheck = new SelfCheckService(_github);
    TextElement.SetFontFamily(this, new FontFamily("Microsoft JhengHei UI, Microsoft YaHei UI, Segoe UI, Noto Sans TC"));
    Title = DevBuild ? "鬼島監控台（開發版）" : "鬼島監控台";
    Icon = new WindowIcon(AssetLoader.Open(new Uri(DevBuild ? "avares://KuidaoMonitor/Assets/kuidao_dev.ico" : "avares://KuidaoMonitor/Assets/kuidao.ico")));
    Width = 1440; Height = 900; MinWidth = 1200; MinHeight = 760;
    Background = Ui.Bg;
    Content = Build();
    var local = new DispatcherTimer { Interval = TimeSpan.FromSeconds(2) };
    local.Tick += (_, _) => PollLocal();
    local.Start();
    var remote = new DispatcherTimer { Interval = TimeSpan.FromSeconds(60) };
    remote.Tick += async (_, _) => await RefreshGitHubAsync();
    var schedule = new DispatcherTimer { Interval = TimeSpan.FromSeconds(20) };
    schedule.Tick += async (_, _) => await TickScheduleAsync();
    schedule.Start();
    UpdateScheduleLabels();
    DispatcherTimer.RunOnce(async () => await RefreshAllAsync(), TimeSpan.FromMilliseconds(800));
    remote.Start();
  }

  private Control Build()
  {
    var root = new Grid();
    root.ColumnDefinitions.Add(new ColumnDefinition(GridLength.Auto));
    root.ColumnDefinitions.Add(new ColumnDefinition(GridLength.Star));

    var rail = new StackPanel
    {
      Width = 76, Background = Brush.Parse("#0F1218"), Spacing = 10,
      HorizontalAlignment = HorizontalAlignment.Center, Margin = new Thickness(0, 22, 0, 16)
    };
    rail.Children.Add(new Border
    {
      Width = 42, Height = 42, CornerRadius = new CornerRadius(13), Background = Ui.Blue, Margin = new Thickness(0, 0, 0, 10),
      Child = Ui.Text("鬼", 18, Brushes.White, true, HorizontalAlignment.Center, VerticalAlignment.Center)
    });
    var labels = new[] { "監控", "網路", "發布", "自檢", "排程", "建議" };
    for (int i = 0; i < labels.Length; i++) rail.Children.Add(MakeNav(labels[i], i));
    Grid.SetColumn(rail, 0); root.Children.Add(rail);

    var main = new Grid { Margin = new Thickness(24) };
    main.RowDefinitions.Add(new RowDefinition(GridLength.Auto));
    main.RowDefinitions.Add(new RowDefinition(GridLength.Star));

    var top = new Grid { Margin = new Thickness(0, 0, 0, 16) };
    top.ColumnDefinitions.Add(new ColumnDefinition(GridLength.Star));
    top.ColumnDefinitions.Add(new ColumnDefinition(GridLength.Auto));
    var title = new StackPanel { Spacing = 3 };
    title.Children.Add(Ui.Text("鬼島 MONITOR", 24, Ui.TextBrush, true));
    title.Children.Add(Ui.Text("本機程序、網路、發布與自檢總覽", 11, Ui.Muted));
    top.Children.Add(title);
    var actions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10, VerticalAlignment = VerticalAlignment.Center };
    var refresh = new Button { Content = "立即自檢", Padding = new Thickness(14, 8), Background = Ui.Blue, Foreground = Brushes.White, CornerRadius = new CornerRadius(10) };
    refresh.Click += async (_, _) => await RefreshAllAsync();
    _updated.MaxWidth = 320;
    _updated.VerticalAlignment = VerticalAlignment.Center;
    actions.Children.Add(refresh); actions.Children.Add(_updated);
    Grid.SetColumn(actions, 1); top.Children.Add(actions);
    Grid.SetRow(top, 0); main.Children.Add(top);

    _pages.Add(BuildMonitorPage());
    _pages.Add(BuildNetworkPage());
    _pages.Add(BuildReleasePage());
    _pages.Add(BuildSelfCheckPage());
    _pages.Add(BuildSchedulePage());
    _pages.Add(BuildSuggestionsPage());
    Grid.SetRow(_contentHost, 1); main.Children.Add(_contentHost);

    // 首次載入前的佔位提示，避免一開起來就有空卡。
    ShowRemoteEmptyStates();
    _checkPanel.Children.Add(Ui.Empty("等待自檢結果…"));
    _suggestionPanel.Children.Add(Ui.Empty("等待自檢結果…"));

    Grid.SetColumn(main, 1); root.Children.Add(main);
    SelectTab(0);
    return root;
  }

  // ---------- 分頁 ----------

  private Button MakeNav(string label, int index)
  {
    var b = new Button
    {
      Content = label,
      Width = 62, MinHeight = 46,
      Padding = new Thickness(0),
      HorizontalContentAlignment = HorizontalAlignment.Center,
      VerticalContentAlignment = VerticalAlignment.Center,
      Background = Brushes.Transparent,
      Foreground = Ui.Muted,
      BorderThickness = new Thickness(0),
      CornerRadius = new CornerRadius(12),
      Tag = index
    };
    b.Click += (_, _) => SelectTab((int)b.Tag!);
    _navButtons.Add(b);
    return b;
  }

  private void SelectTab(int index)
  {
    for (int i = 0; i < _navButtons.Count; i++)
    {
      var b = _navButtons[i];
      bool selected = i == index;
      b.Background = selected ? Brush.Parse("#1B2A44") : Brushes.Transparent;
      b.Foreground = selected ? Ui.TextBrush : Ui.Muted;
      b.FontWeight = selected ? FontWeight.SemiBold : FontWeight.Normal;
    }
    _contentHost.Children.Clear();
    _contentHost.Children.Add(_pages[index]);
  }

  private Control BuildMonitorPage()
  {
    var page = new Grid();
    page.RowDefinitions.Add(new RowDefinition(GridLength.Auto));
    page.RowDefinitions.Add(new RowDefinition(GridLength.Star));

    var stats = new Grid { ColumnDefinitions = new ColumnDefinitions("*,*,*,*"), Margin = new Thickness(0, 0, 0, 16) };
    AddStat(stats, 0, "執行程序", _statProcesses, "本機鬼島相關 PID");
    AddStat(stats, 1, "指令事件", _statCommands, "啟動／結束記錄");
    AddStat(stats, 2, "下載總數", _statDownloads, "Release 資產累計");
    AddStat(stats, 3, "倉庫流量", _statRepo, "Views / Clones");
    Grid.SetRow(stats, 0); page.Children.Add(stats);

    var command = Ui.Card("鬼島執行指令", out var cmdBody, "即時");
    cmdBody.Children.Add(_commandPanel);
    Grid.SetRow(command, 1); page.Children.Add(command);
    return page;
  }

  private Control BuildNetworkPage()
  {
    var net = Ui.Card("網路流量", out var netBody, "即時");
    var netStats = new Grid { ColumnDefinitions = new ColumnDefinitions("*,*"), Margin = new Thickness(0, 0, 0, 4) };
    netStats.Children.Add(Labeled("全系統", _systemNet));
    netStats.Children.Add(Labeled("鬼島程序", _kuidaoNet));
    Grid.SetColumn(netStats.Children[1], 1);
    netBody.Children.Add(netStats);
    netBody.Children.Add(_spark);
    netBody.Children.Add(_connectionPanel);
    return net;
  }

  private Control BuildReleasePage()
  {
    var g = new Grid { ColumnDefinitions = new ColumnDefinitions("*,*") };
    var release = Ui.Card("網站與發布", out var releaseBody, "GitHub");
    releaseBody.Children.Add(_releasePanel);
    g.Children.Add(release);
    var downloads = Ui.Card("下載次數", out var dlBody, "資產");
    dlBody.Children.Add(_downloadPanel);
    downloads.Margin = new Thickness(14, 0, 0, 0);
    Grid.SetColumn(downloads, 1); g.Children.Add(downloads);
    return g;
  }

  private Control BuildSelfCheckPage()
  {
    var check = Ui.Card("自檢模式", out var checkBody, "進度");
    checkBody.Children.Add(_checkPanel);
    return check;
  }

  private Control BuildSchedulePage()
  {
    var g = new Grid { ColumnDefinitions = new ColumnDefinitions("*,*") };

    var auto = Ui.Card("全自動排程", out var autoBody, "時間到自動執行");
    autoBody.Children.Add(Ui.Text("關閉後不會自動執行，只能用右側按鈕手動介入。", 10, Ui.Muted));
    _autoToggle.IsChecked = _schedule.AutoEnabled;
    _autoToggle.Foreground = Ui.TextBrush;
    _autoToggle.Checked += (_, _) => { _schedule.AutoEnabled = true; _schedule.Save(); UpdateScheduleLabels(); };
    _autoToggle.Unchecked += (_, _) => { _schedule.AutoEnabled = false; _schedule.Save(); UpdateScheduleLabels(); };
    autoBody.Children.Add(_autoToggle);

    var selfRow = new Grid { ColumnDefinitions = new ColumnDefinitions("140,*") };
    selfRow.Children.Add(Ui.Text("自檢時間（HH:mm）", 10.5, Ui.TextBrush));
    StyleTimeBox(_selfTime, _schedule.SelfCheckTime);
    Grid.SetColumn(_selfTime, 1); selfRow.Children.Add(_selfTime);
    autoBody.Children.Add(selfRow);

    var updateRow = new Grid { ColumnDefinitions = new ColumnDefinitions("140,*") };
    updateRow.Children.Add(Ui.Text("更新檢查時間（HH:mm）", 10.5, Ui.TextBrush));
    StyleTimeBox(_updateTime, _schedule.UpdateTime);
    Grid.SetColumn(_updateTime, 1); updateRow.Children.Add(_updateTime);
    autoBody.Children.Add(updateRow);

    _selfTime.LostFocus += (_, _) => CommitTimes();
    _updateTime.LostFocus += (_, _) => CommitTimes();
    _selfTime.KeyDown += (_, e) => { if (e.Key == Avalonia.Input.Key.Enter) CommitTimes(); };
    _updateTime.KeyDown += (_, e) => { if (e.Key == Avalonia.Input.Key.Enter) CommitTimes(); };

    autoBody.Children.Add(Ui.Text("格式 00:30，也可輸入 0030；只看本機時間。", 9.5, Ui.Muted));
    autoBody.Children.Add(_nextRun);
    g.Children.Add(auto);

    var manual = Ui.Card("手動介入", out var manualBody, "立即執行");
    manualBody.Children.Add(Ui.Text("不用等排程，按下去就立刻跑一輪。", 10, Ui.Muted));
    var buttons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10 };
    var runSelf = new Button { Content = "立即自檢", Padding = new Thickness(14, 8), Background = Ui.Blue, Foreground = Brushes.White, CornerRadius = new CornerRadius(10) };
    runSelf.Click += async (_, _) => await RunSelfCheckNowAsync();
    var runUpdate = new Button { Content = "立即檢查更新", Padding = new Thickness(14, 8), Background = Ui.Green, Foreground = Brushes.White, CornerRadius = new CornerRadius(10) };
    runUpdate.Click += async (_, _) => await RunUpdateNowAsync();
    buttons.Children.Add(runSelf); buttons.Children.Add(runUpdate);
    manualBody.Children.Add(buttons);
    manualBody.Children.Add(Ui.Text("最近執行", 10, Ui.Muted));
    manualBody.Children.Add(_scheduleStatus);
    manualBody.Children.Add(Ui.Text("自動排程關閉時，這裡仍可隨時手動跑；跑完會更新自檢與發布兩頁。", 9.5, Ui.Muted));
    manual.Margin = new Thickness(14, 0, 0, 0);
    Grid.SetColumn(manual, 1); g.Children.Add(manual);
    return g;
  }

  private static void StyleTimeBox(TextBox box, string value)
  {
    box.Text = value;
    box.Padding = new Thickness(10, 6);
    box.CornerRadius = new CornerRadius(8);
    box.Background = Brush.Parse("#10141B");
    box.Foreground = Ui.TextBrush;
    box.BorderBrush = Ui.Line;
  }

  private void CommitTimes()
  {
    var selfOk = ScheduleSettings.TryParseTime(_selfTime.Text, out var sh, out var sm);
    var updateOk = ScheduleSettings.TryParseTime(_updateTime.Text, out var uh, out var um);
    if (selfOk) _schedule.SelfCheckTime = $"{sh:00}:{sm:00}";
    else _selfTime.Text = _schedule.SelfCheckTime;
    if (updateOk) _schedule.UpdateTime = $"{uh:00}:{um:00}";
    else _updateTime.Text = _schedule.UpdateTime;
    _schedule.Save();
    UpdateScheduleLabels();
  }

  private void UpdateScheduleLabels()
  {
    var now = DateTime.Now;
    _nextRun.Text = _schedule.AutoEnabled
      ? $"下次自檢：{ScheduleSettings.Describe(_schedule.NextSelfCheck(now))}｜下次更新檢查：{ScheduleSettings.Describe(_schedule.NextUpdate(now))}"
      : "自動排程已關閉，請用手動按鈕執行";
    var parts = new List<string>();
    if (!string.IsNullOrEmpty(_schedule.LastSelfCheckAt)) parts.Add($"自檢 {_schedule.LastSelfCheckAt}");
    if (!string.IsNullOrEmpty(_schedule.LastUpdateAt)) parts.Add($"更新檢查 {_schedule.LastUpdateAt}");
    _scheduleStatus.Text = parts.Count == 0 ? "尚未執行" : string.Join("｜", parts);
    _scheduleStatus.Foreground = parts.Count == 0 ? Ui.Muted : Ui.TextBrush;
  }

  private async Task TickScheduleAsync()
  {
    var now = DateTime.Now;
    var today = now.ToString("yyyy-MM-dd");
    if (_schedule.AutoEnabled && _schedule.SelfCheckValid
        && ScheduleSettings.TryParseTime(_schedule.SelfCheckTime, out var sh, out var sm)
        && now.Hour == sh && now.Minute == sm && _schedule.LastSelfCheckDate != today)
    {
      _schedule.LastSelfCheckDate = today;
      _schedule.Save();
      await RunSelfCheckNowAsync();
    }
    if (_schedule.AutoEnabled && _schedule.UpdateValid
        && ScheduleSettings.TryParseTime(_schedule.UpdateTime, out var uh, out var um)
        && now.Hour == uh && now.Minute == um && _schedule.LastUpdateDate != today)
    {
      _schedule.LastUpdateDate = today;
      _schedule.Save();
      await RunUpdateNowAsync();
    }
    UpdateScheduleLabels();
  }

  private async Task RunSelfCheckNowAsync()
  {
    if (_selfCheckRunning) return;
    _selfCheckRunning = true;
    _scheduleStatus.Text = "自檢執行中…";
    _scheduleStatus.Foreground = Ui.Blue;
    try
    {
      await RefreshAllAsync();
      _schedule.LastSelfCheckAt = DateTime.Now.ToString("MM-dd HH:mm");
      _schedule.Save();
      _scheduleStatus.Text = $"自檢完成 {DateTime.Now:HH:mm:ss}";
      _scheduleStatus.Foreground = Ui.Green;
    }
    catch (Exception ex)
    {
      _scheduleStatus.Text = $"自檢失敗：{ex.Message}";
      _scheduleStatus.Foreground = Ui.Red;
    }
    finally
    {
      _selfCheckRunning = false;
      UpdateScheduleLabels();
    }
  }

  private async Task RunUpdateNowAsync()
  {
    _scheduleStatus.Text = "更新檢查執行中…";
    _scheduleStatus.Foreground = Ui.Blue;
    try
    {
      await RefreshGitHubAsync();
      _schedule.LastUpdateAt = DateTime.Now.ToString("MM-dd HH:mm");
      _schedule.Save();
      _scheduleStatus.Text = $"更新檢查完成 {DateTime.Now:HH:mm:ss}";
      _scheduleStatus.Foreground = Ui.Green;
    }
    catch (Exception ex)
    {
      _scheduleStatus.Text = $"更新檢查失敗：{ex.Message}";
      _scheduleStatus.Foreground = Ui.Red;
    }
    finally
    {
      UpdateScheduleLabels();
    }
  }

  private Control BuildSuggestionsPage()
  {
    var suggest = Ui.Card("建議新增與修復", out var sugBody, "優先");
    sugBody.Children.Add(_suggestionPanel);
    return suggest;
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
      _githubFailed = false;
      _updated.Text = $"更新於 {DateTime.Now:HH:mm:ss}";
      _updated.Foreground = Ui.Muted;
      UpdateGitHubPanels();
    }
    catch
    {
      _githubFailed = true;
      _updated.Text = $"連線失敗 {DateTime.Now:HH:mm:ss}";
      _updated.Foreground = Ui.Red;
      ShowRemoteEmptyStates();
    }
  }

  private void ShowRemoteEmptyStates()
  {
    var hint = _githubFailed ? "連線失敗，請稍後重試或檢查網路、代理與 API 額度" : "等待 GitHub 資料…";
    _releasePanel.Children.Clear();
    _releasePanel.Children.Add(Ui.Empty(hint));
    _downloadPanel.Children.Clear();
    _downloadPanel.Children.Add(Ui.Empty(hint));
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
