using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Media;

namespace KuidaoMonitor;

public static class Ui
{
  public static readonly IBrush Bg = Brush.Parse("#0B0D10");
  public static readonly IBrush CardBg = Brush.Parse("#14181F");
  public static readonly IBrush CardBg2 = Brush.Parse("#181D25");
  public static readonly IBrush Line = Brush.Parse("#242B35");
  public static readonly IBrush Blue = Brush.Parse("#2F80FF");
  public static readonly IBrush Green = Brush.Parse("#27D98F");
  public static readonly IBrush Orange = Brush.Parse("#FFB020");
  public static readonly IBrush Red = Brush.Parse("#FF5A5F");
  public static readonly IBrush Muted = Brush.Parse("#8A93A3");
  public static readonly IBrush TextBrush = Brush.Parse("#F2F5FA");

  public static Border Card(string title, out StackPanel body, string? badge = null)
  {
    body = new StackPanel { Spacing = 10 };
    var head = new Grid();
    head.ColumnDefinitions.Add(new ColumnDefinition(GridLength.Star));
    head.ColumnDefinitions.Add(new ColumnDefinition(GridLength.Auto));
    head.Children.Add(Text(title, 15, TextBrush, true));
    if (badge != null)
    {
      var pill = new Border { Background = Brush.Parse("#1B2A44"), CornerRadius = new CornerRadius(999), Padding = new Thickness(9, 3), Child = Text(badge, 10, Blue) };
      Grid.SetColumn(pill, 1); head.Children.Add(pill);
    }
    body.Children.Add(head);
    return new Border
    {
      Background = CardBg,
      BorderBrush = Line,
      BorderThickness = new Thickness(1),
      CornerRadius = new CornerRadius(16),
      Padding = new Thickness(16),
      Child = body
    };
  }

  public static Border StatCard(string label, TextBlock value, string sub, IBrush accent)
  {
    var panel = new StackPanel { Spacing = 7 };
    panel.Children.Add(Text(label, 10.5, Muted));
    panel.Children.Add(value);
    panel.Children.Add(Text(sub, 10, accent));
    return new Border { Background = CardBg, BorderBrush = Line, BorderThickness = new Thickness(1), CornerRadius = new CornerRadius(16), Padding = new Thickness(16), Child = panel };
  }

  public static Border StatCard(string label, string value, string sub, IBrush accent) =>
    StatCard(label, Text(value, 27, TextBrush, true), sub, accent);

  public static TextBlock Text(string s, double size = 12, IBrush? brush = null, bool bold = false,
    HorizontalAlignment? horizontal = null, VerticalAlignment? vertical = null)
  {
    var t = new TextBlock
    {
      Text = s,
      FontSize = size,
      Foreground = brush ?? Muted,
      FontWeight = bold ? FontWeight.Bold : FontWeight.Normal,
      TextWrapping = TextWrapping.NoWrap,
      TextTrimming = TextTrimming.CharacterEllipsis
    };
    if (horizontal.HasValue) t.HorizontalAlignment = horizontal.Value;
    if (vertical.HasValue) t.VerticalAlignment = vertical.Value;
    return t;
  }

  public static ProgressBar Progress(double value, IBrush? brush = null)
  {
    var p = new ProgressBar
    {
      Minimum = 0, Maximum = 100, Value = Math.Clamp(value, 0, 100),
      Height = 7, CornerRadius = new CornerRadius(99),
      Background = Brush.Parse("#232933"),
      Foreground = brush ?? Blue
    };
    return p;
  }

  public static Border Pill(string s, IBrush color) => new()
  {
    Background = color, CornerRadius = new CornerRadius(999), Padding = new Thickness(9, 3),
    Child = Text(s, 10, color == Blue ? Brush.Parse("#DCE9FF") : Brush.Parse("#E8FFF6"))
  };
}

public sealed class DualSparkline : Control
{
  public IReadOnlyList<double> A { get; private set; } = Array.Empty<double>();
  public IReadOnlyList<double> B { get; private set; } = Array.Empty<double>();
  public void Update(IReadOnlyList<double> a, IReadOnlyList<double> b) { A = a; B = b; InvalidateVisual(); }

  public override void Render(DrawingContext context)
  {
    context.DrawRectangle(Brush.Parse("#10141B"), null, new Avalonia.Rect(Bounds.Size), 12, 12);
    Draw(context, A, Ui.Blue, Bounds);
    Draw(context, B, Ui.Green, Bounds);
  }

  private static void Draw(DrawingContext context, IReadOnlyList<double> values, IBrush brush, Rect bounds)
  {
    if (values.Count < 2) return;
    var max = Math.Max(1, values.Max());
    var geo = new StreamGeometry();
    using (var ctx = geo.Open())
    {
      for (int i = 0; i < values.Count; i++)
      {
        var x = i * (bounds.Width / Math.Max(1, values.Count - 1));
        var y = bounds.Height - 6 - values[i] / max * (bounds.Height - 14);
        var p = new Avalonia.Point(x, y);
        if (i == 0) ctx.BeginFigure(p, false); else ctx.LineTo(p);
      }
      ctx.EndFigure(false);
    }
    context.DrawGeometry(null, new Pen(brush, 2), geo);
  }
}
