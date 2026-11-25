# Kiro 額度適配

通過本機 Kiro 登錄緩存查詢 GetUsageLimits。只讀登錄文件，不儲存令牌副本，不回寫，不使用 refreshToken 自動續期。

## 登錄與身份

默認路徑為 `%USERPROFILE%\.aws\sso\cache\kiro-auth-token.json`，也可選擇其他登錄 JSON。連接與重連在後台核驗成功、配置儲存成功後生效。取消或關閉窗口後，晚到結果不會儲存帳號。

- `accessToken`：Bearer 憑據，僅發往下列固定官方端點。
- `profileArn`：有值時隨請求發送；缺少時允許空請求體，兼容企業 IdC 登錄。
- `region`：與 ARN 區域一致時使用；缺少時從 ARN 提取，兩者都缺少時使用美國東部區域。
- `expiresAt`：RFC3339 格式的客戶端續期時間，登錄失效以服務端 401 / 403 為準。
- `userInfo.userId`：使用響應中的用戶標識綁定帳號，配置和緩存僅儲存其 SHA-256 摘要。缺少可核驗用戶標識時不顯示額度。

客戶端註冊 `clientIdHash`、profile ARN 和訪問令牌均不作為用戶身份。原版本尚未核驗的身份需要重新連接；已經核驗的帳號重連時必須仍是原用戶。重連保留帳號 ID、備註、配色及常駐設置。

## 請求邊界

| 區域 | 固定端點 |
| --- | --- |
| us-east-1 | https://codewhisperer.us-east-1.amazonaws.com/ |
| eu-central-1 | https://q.eu-central-1.amazonaws.com/ |

其他區域暫不支援，不根據登錄文件拼接任意主機。請求使用 POST、`application/x-amz-json-1.0` 和 `X-Amz-Target: AmazonCodeWhispererService.GetUsageLimits`。禁用重定向，使用既有代理與請求超時；解壓後響應上限為 1 MiB。

401 / 403 提示重新登錄；429 保留服務端 Retry-After 並按帳號退避。查詢期間登錄文件發生變化時丟棄結果。Kiro 請求獨立執行，避免阻塞其他額度服務。

## 點數與時間

從 `usageBreakdownList` 的 CREDIT 項讀取套餐用量。優先精度字段，缺少精度字段時兼容對應整數；負數、非有限值及損壞字段不按零處理。

- 月度套餐：總額減去套餐內消耗。套餐內消耗使用總用量減超額用量，不因贈送列表是否非空切換公式。
- 試用：僅計入 ACTIVE 且未過期的 `freeTrialInfo`。
- 贈送 / 加購：僅計入 ACTIVE 且未過期的 `bonuses`，兼容單個對象和數組，逐項保留名稱及到期時間。
- 可用點數：套餐、有效試用和有效贈送 / 加購的剩餘量合計。
- 超額上限餘量：啟用超額且 CREDIT 項返回明確點數上限時單獨顯示，不加進可用點數；不把 `capForWorkloadUsage` 等金額字段換算成點數。

月度重置與試用、贈送的到期時間分別顯示。秒與毫秒時間戳均做範圍校驗。額度環中心顯示 Kiro 標識，剩餘點數顯示在卡片右側及常駐欄圖標右側；設置頁保留各積分池明細。舊的其他供應商緩存可以繼續讀取。

## 參考

- [Kiro 計費與試用](https://kiro.dev/docs/billing/)
- [Kiro 加購點數](https://kiro.dev/docs/billing/add-on-credits/)
- [AWS 官方 GetUsageLimits 響應模型](https://github.com/aws/amazon-q-developer-cli/blob/15cc8f3cd18c4272925ce1c7053268eedff1ea0a/crates/amzn-codewhisperer-client/src/operation/get_usage_limits/_get_usage_limits_output.rs)
- [AWS 官方用量字段模型](https://github.com/aws/amazon-q-developer-cli/blob/15cc8f3cd18c4272925ce1c7053268eedff1ea0a/crates/amzn-codewhisperer-client/src/types/_usage_breakdown.rs)

真實帳號還需核對 Kiro 客戶端中的點數、用戶身份、區域及到期時間。合成測試不代表所有登錄形態均已完成真機驗收。