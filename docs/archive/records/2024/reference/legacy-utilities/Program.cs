using Avalonia;

namespace KuidaoMonitor;

public static class Program
{
  public static void Main(string[] args) => BuildAvaloniaApp().StartWithClassicDesktopLifetime(args);
  public static AppBuilder BuildAvaloniaApp() => AppBuilder.Configure<App>().UsePlatformDetect().LogToTrace();
}

public class App : Application
{
  public override void OnFrameworkInitializationCompleted()
  {
    if (ApplicationLifetime is Avalonia.Controls.ApplicationLifetimes.IClassicDesktopStyleApplicationLifetime desktop)
      desktop.MainWindow = new UnlockWindow();
    base.OnFrameworkInitializationCompleted();
  }
}
