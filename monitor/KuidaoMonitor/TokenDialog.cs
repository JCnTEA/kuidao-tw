using System.Diagnostics;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Media;

namespace KuidaoMonitor;

public sealed class TokenDialog : Window
{
  private readonly TextBox _input = new()
  {
    PasswordChar = '●',
    Watermark = "貼上 ghp_ 或 github_pat_ 開頭的權杖",
    FontSize = 12,
    Padding = new Thickness(10, 8),
    Background = Brush.Parse("#0F141B"),
    Foreground = Ui.TextBrush,
    BorderBrush = Ui.Line,
    BorderThickness = new Thickness(1),
    CornerRadius = new CornerRadius(9)
  };
  private readonly CheckBox _show = new()
  {
    Content = Ui.Text("顯示權杖", 10.5),
    Foreground = Ui.Muted,
    IsChecked = false
  };

  private TokenDialog(string? current)
  {
    Title = "設定 GitHub Token";
    Width = 460; Height = 300;
    CanResize = false;
    Background = Ui.Bg;
    WindowStartupLocation = WindowStartupLocation.CenterOwner;

    var panel = new StackPanel { Margin = new Thickness(22), Spacing = 12 };
    panel.Children.Add(Ui.Text("設定 GitHub Token", 17, Ui.TextBrush, true));
    panel.Children.Add(Ui.Text("倉庫流量（Views / Clones）需要具備 repo 權限的權杖；權杖只會儲存在本機，不會上傳其他位置。", 10.5));
    panel.Children.Add(_input);
    if (!string.IsNullOrWhiteSpace(current)) _input.Text = current;

    var options = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 14 };
    _show.Click += (_, _) => _input.PasswordChar = _show.IsChecked == true ? '\0' : '●';
    options.Children.Add(_show);
    var openLink = Ui.SmallButton("前往 GitHub 建立權杖", Brush.Parse("#1B2230"), Ui.Blue);
    openLink.Click += (_, _) =>
    {
      try
      {
        Process.Start(new ProcessStartInfo("https://github.com/settings/tokens/new?scopes=repo&description=KuidaoMonitor") { UseShellExecute = true });
      }
      catch { }
    };
    options.Children.Add(openLink);
    panel.Children.Add(options);

    var buttons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10, HorizontalAlignment = HorizontalAlignment.Right, Margin = new Thickness(0, 6, 0, 0) };
    var clear = Ui.GhostButton("清除權杖");
    var cancel = Ui.GhostButton("取消");
    var save = Ui.SmallButton("儲存並重新整理");
    buttons.Children.Add(clear); buttons.Children.Add(cancel); buttons.Children.Add(save);
    panel.Children.Add(buttons);

    clear.Click += (_, _) => Close(string.Empty);
    cancel.Click += (_, _) => Close(null);
    save.Click += (_, _) => Close(_input.Text?.Trim() ?? string.Empty);

    Content = panel;
    Opened += (_, _) => _input.Focus();
    KeyDown += (_, ke) =>
    {
      if (ke.Key == Avalonia.Input.Key.Escape) Close(null);
      else if (ke.Key == Avalonia.Input.Key.Enter) Close(_input.Text?.Trim() ?? string.Empty);
    };
  }

  /// <summary>開啟對話框；回傳 null 表示取消，string.Empty 表示清除，其餘為新權杖。</summary>
  public static Task<string?> ShowAsync(Window owner, string? current)
  {
    var dialog = new TokenDialog(current);
    return dialog.ShowDialog<string?>(owner);
  }
}
