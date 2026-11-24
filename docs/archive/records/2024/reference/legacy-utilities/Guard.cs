using System;
using System.IO;
using System.Text.Json;

namespace KuidaoMonitor;

/// <summary>與「存取碼」產生器共用的 30 秒一次性通行碼。</summary>
public static class Guard
{
  private sealed class Payload
  {
    public string Code { get; set; } = "";
    public long Issued { get; set; }
    public long Expires { get; set; }
    public bool Used { get; set; }
  }

  private static string CodePath => Path.Combine(
    Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "KuidaoGuard", "code.json");

  public static bool TryConsume(string input, out string message)
  {
    message = "";
    var code = input.Trim();
    if (code.Length != 6 || !int.TryParse(code, out _)) { message = "請輸入 6 位數字"; return false; }
    try
    {
      if (!File.Exists(CodePath)) { message = "尚未產生通行碼，請先開啟「存取碼」"; return false; }
      var payload = JsonSerializer.Deserialize<Payload>(File.ReadAllText(CodePath));
      if (payload is null) { message = "通行碼檔案異常"; return false; }
      var now = DateTimeOffset.UtcNow.ToUnixTimeSeconds();
      if (payload.Used) { message = "此通行碼已使用過，請重新產生"; return false; }
      if (now > payload.Expires) { message = "通行碼已過期，請重新產生"; return false; }
      if (!string.Equals(payload.Code, code, StringComparison.Ordinal)) { message = "通行碼不正確"; return false; }
      payload.Used = true;
      var tmp = CodePath + ".tmp";
      File.WriteAllText(tmp, JsonSerializer.Serialize(payload));
      File.Move(tmp, CodePath, true);
      return true;
    }
    catch
    {
      message = "無法讀取通行碼";
      return false;
    }
  }
}
