// 站點地址：https://api.trae.cn。API Key 填 Cookie 請求頭的值，Access Token 填 Cloud-IDE-JWT 後的令牌。
(() => {
  const cookie = "{{apiKey}}".trim();
  const token = "{{accessToken}}".trim();
  if (!cookie || !token) throw new Error("請填寫 TRAE 國區的 Cookie 和短期令牌");

  return {
    request: {
      url: "https://api.trae.cn/trae/api/v2/pay/user_current_entitlement_list",
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Origin": "https://www.trae.cn",
        "Referer": "https://www.trae.cn/",
        "Cookie": cookie,
        "Authorization": `Cloud-IDE-JWT ${token}`
      },
      body: JSON.stringify({ require_usage: true, full_data: true })
    },
    extractor(response) {
      const total = response?.usage_summary?.total_amount;
      const used = response?.usage_summary?.consumed_amount;
      if (response?.is_credits_billing !== true ||
          !Number.isFinite(total) || total < 0 || !Number.isFinite(used) || used < 0) {
        return { isValid: false };
      }
      return { planName: "總積分", unit: "積分", remaining: Math.max(0, total - used), total, used };
    }
  };
})()
