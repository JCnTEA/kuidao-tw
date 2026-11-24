using System.Text.Json;

namespace KuidaoMonitor;

/// <summary>監控台的自檢／更新排程設定；關閉時只接受手動執行。</summary>
public sealed class ScheduleSettings
{
  public bool AutoEnabled { get; set; } = true;
  public string SelfCheckTime { get; set; } = "00:30";
  public string UpdateTime { get; set; } = "01:00";
  public string LastSelfCheckDate { get; set; } = "";
  public string LastUpdateDate { get; set; } = "";
  public string LastSelfCheckAt { get; set; } = "";
  public string LastUpdateAt { get; set; } = "";

  private static string Folder =>
    Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "KuidaoMonitor");

  private static string FilePath => Path.Combine(Folder, "schedule.json");

  public static ScheduleSettings Load()
  {
    try
    {
      if (File.Exists(FilePath))
        return JsonSerializer.Deserialize<ScheduleSettings>(File.ReadAllText(FilePath)) ?? new ScheduleSettings();
    }
    catch
    {
      // 設定檔損壞時回到預設值，不讓監控台因此開不起來。
    }
    return new ScheduleSettings();
  }

  public void Save()
  {
    try
    {
      Directory.CreateDirectory(Folder);
      File.WriteAllText(FilePath, JsonSerializer.Serialize(this, new JsonSerializerOptions { WriteIndented = true }));
    }
    catch
    {
      // 寫入失敗只影響下次啟動的記憶，不影響目前排程。
    }
  }

  /// <summary>接受 0:30、00:30、0030、00：30；超出範圍一律視為無效。</summary>
  public static bool TryParseTime(string? text, out int hour, out int minute)
  {
    hour = 0; minute = 0;
    var raw = (text ?? "").Trim().Replace('：', ':').Replace('．', ':');
    if (raw.Length == 4 && raw.All(char.IsDigit))
    {
      raw = raw[..2] + ":" + raw[2..];
    }
    var parts = raw.Split(':');
    if (parts.Length != 2) return false;
    if (!int.TryParse(parts[0], out hour) || !int.TryParse(parts[1], out minute)) return false;
    return hour is >= 0 and <= 23 && minute is >= 0 and <= 59;
  }

  public bool SelfCheckValid => TryParseTime(SelfCheckTime, out _, out _);
  public bool UpdateValid => TryParseTime(UpdateTime, out _, out _);

  /// <summary>今天（或明天）的實際執行時間；設定無效時回傳 null。</summary>
  public DateTime? NextSelfCheck(DateTime now) => NextAt(now, SelfCheckTime);
  public DateTime? NextUpdate(DateTime now) => NextAt(now, UpdateTime);

  private static DateTime? NextAt(DateTime now, string text)
  {
    if (!TryParseTime(text, out var h, out var m)) return null;
    var today = new DateTime(now.Year, now.Month, now.Day, h, m, 0);
    return today > now ? today : today.AddDays(1);
  }

  public static string Describe(DateTime? at) =>
    at is null ? "時間設定無效" : at.Value.Date == DateTime.Today
      ? $"今天 {at.Value:HH:mm}"
      : $"明天 {at.Value:HH:mm}";
}
