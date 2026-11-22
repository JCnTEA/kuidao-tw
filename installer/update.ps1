param([Parameter(Mandatory=$true)][string]$WorkPath, [Parameter(Mandatory=$true)][string]$TargetExe, [int]$ParentProcessId)
$ErrorActionPreference = 'Stop'
# 更新只使用目前 PowerShell 自帶模組，避免繼承其他終端版本的模組路徑。
$env:PSModulePath = [IO.Path]::Combine($PSHOME, 'Modules')
$cacheRoot = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'Kuidao\Updates')).TrimEnd('\')
$workRoot = [IO.Path]::GetFullPath($WorkPath).TrimEnd('\')
$target = [IO.Path]::GetFullPath($TargetExe)
$targetDir = Split-Path $target -Parent
$tag = [Guid]::NewGuid().ToString('N')
$staged = Join-Path $targetDir ('kuidao-update-' + $tag + '.exe')
$backup = Join-Path $workRoot 'previous.exe'
$assetsStaged = Join-Path $targetDir ('assets-update-' + $tag)
$assetsBackup = Join-Path $workRoot 'assets-previous'
$assetsDir = Join-Path $targetDir 'assets'
$replaced = $false
$assetsSwapped = $false
$exited = $false
function Assert-PlainPath([string]$PathToCheck) {
    $ancestor = [IO.Path]::GetFullPath($PathToCheck)
    while ($ancestor) {
        if ((Test-Path -LiteralPath $ancestor) -and ((Get-Item -LiteralPath $ancestor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw '更新目錄不能包含目錄連結。' }
        $ancestor = Split-Path $ancestor -Parent
    }
}
function Assert-ZipEntryPath([string]$EntryPath) {
    $normalized = $EntryPath.Replace('\','/')
    if ($normalized.StartsWith('/') -or $normalized -match '^[A-Za-z]:' -or (($normalized -split '/') -contains '..')) { throw '更新包內含無效路徑。' }
}
try {
    if (-not $workRoot.StartsWith($cacheRoot + '\', [StringComparison]::OrdinalIgnoreCase)) { throw '更新快取目錄無效。' }
    Assert-PlainPath $workRoot
    Assert-PlainPath $target
    if (-not (Test-Path -LiteralPath $target -PathType Leaf)) { throw '找不到目前程式。' }
    # version.txt 可能是 `0.6.67` 或帶台灣後綴的 `0.6.67-tw`，只取前面的數字段。
    $raw = (Get-Content -LiteralPath (Join-Path $workRoot 'version.txt') -Raw).Trim()
    if ($raw -notmatch '^v?(\d+\.\d+\.\d+)') { throw '更新版本號無效。' }
    $version = $Matches[1]
    $archive = Join-Path $workRoot 'update.zip'
    $expectedHash = (Get-Content -LiteralPath (Join-Path $workRoot 'hash.txt') -Raw).Trim()
    if ($expectedHash -notmatch '^[a-fA-F0-9]{64}$' -or (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $expectedHash) { throw '更新包校驗失敗。' }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($archive)
    try {
        # 台灣版發佈包：程式在壓縮檔根目錄或單一資料夾裡，檔名固定為 kuidao.exe。
        $entries = @($zip.Entries | Where-Object { ($_.FullName.Replace('\','/') -split '/')[-1] -ceq 'kuidao.exe' })
        if ($entries.Count -ne 1 -or $entries[0].Length -gt 200MB) { throw '更新包中缺少唯一的鬼島程式。' }
        $entry = $entries[0]
        $entryPath = $entry.FullName.Replace('\','/')
        Assert-ZipEntryPath $entryPath
        $prefix = $entryPath.Substring(0, $entryPath.Length - 'kuidao.exe'.Length)
        [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $staged, $true)
        $actual = [Diagnostics.FileVersionInfo]::GetVersionInfo($staged).FileVersion
        if ($actual -ne $version -and $actual -ne "$version.0") { throw '更新包版本與發佈版本不符。' }
        # 吉祥物圖集等資產跟著程式一起換，否則只更新 exe 會留著舊圖。
        $assetEntries = @($zip.Entries | Where-Object { $_.Name -and $_.FullName.Replace('\','/').StartsWith($prefix + 'assets/', [StringComparison]::OrdinalIgnoreCase) })
        if ($assetEntries.Count -gt 0) {
            New-Item -ItemType Directory -Force -Path $assetsStaged | Out-Null
            foreach ($item in $assetEntries) {
                $relative = $item.FullName.Replace('\','/').Substring(($prefix + 'assets/').Length)
                Assert-ZipEntryPath $relative
                $destination = Join-Path $assetsStaged ($relative.Replace('/','\'))
                $parent = Split-Path $destination -Parent
                if (-not (Test-Path -LiteralPath $parent)) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
                [IO.Compression.ZipFileExtensions]::ExtractToFile($item, $destination, $true)
            }
        }
    } finally { $zip.Dispose() }
    Set-Content -LiteralPath (Join-Path $workRoot 'ready') -Value 'ok'
    if ($ParentProcessId -gt 0) {
        $parent = Get-Process -Id $ParentProcessId -ErrorAction SilentlyContinue
        if ($parent -and -not $parent.WaitForExit(60000)) { throw '鬼島尚未退出，請稍後再試。' }
    }
    $exited = $true
    # 快取可能啟用 EFS；先複製備份，避免原子替換時跨加密邊界移動舊程式。
    [IO.File]::Copy($target, $backup, $true)
    [IO.File]::Replace($staged, $target, [NullString]::Value)
    $replaced = $true
    if (Test-Path -LiteralPath $assetsStaged) {
        if (Test-Path -LiteralPath $assetsDir) { Move-Item -LiteralPath $assetsDir -Destination $assetsBackup -Force }
        Move-Item -LiteralPath $assetsStaged -Destination $assetsDir -Force
        $assetsSwapped = $true
    }
    Start-Process -FilePath $target -WorkingDirectory $targetDir -WindowStyle Hidden
    $key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Kuidao'
    $registered = Get-ItemProperty -LiteralPath $key -ErrorAction SilentlyContinue
    if ($registered -and [string]::Equals($registered.InstallLocation.TrimEnd('\'), $targetDir, [StringComparison]::OrdinalIgnoreCase)) {
        Set-ItemProperty -LiteralPath $key -Name DisplayVersion -Value $version -ErrorAction SilentlyContinue
        $own = @('kuidao.exe','Uninstall.exe','LICENSE') | ForEach-Object { Join-Path $targetDir $_ } | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf }
        $sizeKb = [int][Math]::Ceiling((($own | ForEach-Object { (Get-Item -LiteralPath $_).Length }) | Measure-Object -Sum).Sum / 1KB)
        if ($sizeKb -gt 0) { Set-ItemProperty -LiteralPath $key -Name EstimatedSize -Value $sizeKb -Type DWord -ErrorAction SilentlyContinue }
    }
    # 只清理已經確認位於專用快取內的本次更新目錄。
    if ($workRoot.StartsWith($cacheRoot + '\', [StringComparison]::OrdinalIgnoreCase)) {
        try { Assert-PlainPath $workRoot; Remove-Item -LiteralPath $workRoot -Recurse -Force } catch { }
    }
} catch {
    $message = $_.Exception.Message
    if ($assetsSwapped -and (Test-Path -LiteralPath $assetsBackup)) {
        try { Remove-Item -LiteralPath $assetsDir -Recurse -Force; Move-Item -LiteralPath $assetsBackup -Destination $assetsDir -Force } catch { }
    }
    if ($replaced -and (Test-Path -LiteralPath $backup)) { [IO.File]::Copy($backup, $target, $true) }
    if (Test-Path -LiteralPath $staged) { Remove-Item -LiteralPath $staged -Force }
    if (Test-Path -LiteralPath $assetsStaged) { Remove-Item -LiteralPath $assetsStaged -Recurse -Force }
    if (-not $exited) {
        if ($workRoot.StartsWith($cacheRoot + '\', [StringComparison]::OrdinalIgnoreCase)) {
            [IO.File]::WriteAllText((Join-Path $workRoot 'error.txt'), $message)
        }
        exit 1
    }
    if ($exited) { Start-Process -FilePath $target -WorkingDirectory $targetDir -WindowStyle Hidden }
    Add-Type -AssemblyName System.Windows.Forms
    [System.Windows.Forms.MessageBox]::Show("更新未完成：$message`r`n原版本已保留，可稍後重試或透過瀏覽器下載。", '鬼島更新') | Out-Null
    exit 1
}
