using System;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Documents;
using Avalonia.Layout;
using Avalonia.Media;

namespace KuidaoMonitor;

public sealed class UnlockWindow : Window
{
  private readonly TextBox _input = new() { MaxLength = 6, FontSize = 28, Width = 190, HorizontalContentAlignment = HorizontalAlignment.Center, Watermark = "000000" };
  private readonly TextBlock _status = new() { FontSize = 12, Foreground = new SolidColorBrush(Color.Parse("#FF8A8A")), TextWrapping = TextWrapping.Wrap };

  public UnlockWindow()
  {
    TextElement.SetFontFamily(this, new FontFamily("Microsoft JhengHei UI, Microsoft YaHei UI, Segoe UI, Noto Sans TC"));
    Title = "解鎖";
    Width = 340; Height = 260; CanResize = false;
    Background = new SolidColorBrush(Color.Parse("#0A1520"));
    var panel = new StackPanel { Spacing = 12, Margin = new Thickness(26) };
    panel.Children.Add(new TextBlock { Text = "鬼島監控台", FontSize = 17, FontWeight = FontWeight.SemiBold, Foreground = new SolidColorBrush(Color.Parse("#D7EEF7")), HorizontalAlignment = HorizontalAlignment.Center });
    panel.Children.Add(new TextBlock { Text = "輸入「存取碼」產生器的 6 位數通行碼", FontSize = 11, Foreground = new SolidColorBrush(Color.Parse("#7E96A3")), HorizontalAlignment = HorizontalAlignment.Center });
    panel.Children.Add(_input);
    panel.Children.Add(_status);
    var ok = new Button { Content = "解鎖", FontSize = 14, Padding = new Thickness(30, 6), HorizontalAlignment = HorizontalAlignment.Center };
    ok.Click += (_, _) => Submit();
    panel.Children.Add(ok);
    Content = panel;
    _input.KeyDown += (_, e) => { if (e.Key == Avalonia.Input.Key.Enter) Submit(); };
    Opened += (_, _) => _input.Focus();
  }

  private void Submit()
  {
    if (Guard.TryConsume(_input.Text ?? "", out var message))
    {
      new MainWindow().Show();
      Close();
      return;
    }
    _status.Text = message;
    _input.Text = "";
    _input.Focus();
  }
}
