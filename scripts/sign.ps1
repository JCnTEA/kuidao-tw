# 共用 Authenticode 簽章腳本。
# 憑證來源（擇一，透過環境變數傳入，避免密碼寫進命令列或紀錄）：
#   CODESIGN_THUMBPRINT   憑證存放區內憑證指紋（CurrentUser\My 或 LocalMachine\My）
#   CODESIGN_PFX          PFX 檔路徑（可搭配 CODESIGN_PFX_PASSWORD）
# 兩者都未設定時直接略過，讓未購置憑證的本機建置照常完成。
param(
    [Parameter(Mandatory = $true)][string[]]$FilePath,
    [string]$TimestampServer = 'http://timestamp.digicert.com',
    [string]$Algorithm = 'SHA256'
)
$ErrorActionPreference = 'Stop'

$thumbprint = $env:CODESIGN_THUMBPRINT
$pfxPath = $env:CODESIGN_PFX
$pfxPassword = $env:CODESIGN_PFX_PASSWORD

$cert = $null
$tempStore = $null
if ($thumbprint) {
    $cert = Get-ChildItem Cert:\CurrentUser\My, Cert:\LocalMachine\My -ErrorAction SilentlyContinue |
        Where-Object { $_.Thumbprint -eq $thumbprint } | Select-Object -First 1
    if (-not $cert) { throw "找不到指紋為 $thumbprint 的程式碼簽章憑證" }
}
elseif ($pfxPath) {
    if (-not (Test-Path -LiteralPath $pfxPath)) { throw "找不到 PFX 檔：$pfxPath" }
    $securePassword = if ($null -ne $pfxPassword) {
        ConvertTo-SecureString $pfxPassword -AsPlainText -Force
    } else {
        New-Object Security.SecureString
    }
    # 匯入暫存的 CurrentUser 存放區，確保私密金鑰可被簽章 API 使用，結束後一併移除。
    $tempStore = New-Object Security.Cryptography.X509Certificates.X509Store('CodeSigningTemp', 'CurrentUser')
    $tempStore.Open('ReadWrite')
    $imported = New-Object Security.Cryptography.X509Certificates.X509Certificate2($pfxPath, $securePassword, [Security.Cryptography.X509Certificates.X509KeyStorageFlags]::PersistKeySet)
    $tempStore.Add($imported)
    $cert = $imported
}
else {
    Write-Output '未設定 CODESIGN_THUMBPRINT / CODESIGN_PFX，略過 Authenticode 簽章。'
    return
}

try {
    foreach ($f in $FilePath) {
        if (-not (Test-Path -LiteralPath $f)) { throw "找不到要簽章的檔案：$f" }
        $done = $false
        for ($attempt = 1; $attempt -le 3 -and -not $done; $attempt++) {
            try {
                $result = Set-AuthenticodeSignature -FilePath $f -Certificate $cert -HashAlgorithm $Algorithm -TimestampServer $TimestampServer
                if ($result.Status -ne 'Valid') { throw "簽章狀態 $($result.Status)：$($result.StatusMessage)" }
                $done = $true
            } catch {
                if ($attempt -eq 3) { throw }
                Start-Sleep -Seconds (2 * $attempt)
            }
        }
        $verify = Get-AuthenticodeSignature $f
        Write-Output ("已簽章：{0} -> {1}" -f [IO.Path]::GetFileName($f), $verify.Status)
    }
}
finally {
    if ($tempStore) {
        $tempStore.Certificates | ForEach-Object { $tempStore.Remove($_) }
        $tempStore.Close()
    }
}
