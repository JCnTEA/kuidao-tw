param([Parameter(Mandatory=$true)][string]$WorkPath, [Parameter(Mandatory=$true)][string]$TargetExe, [int]$ParentProcessId)
$ErrorActionPreference = 'Stop'
# 開發版專用換版腳本：等主程序退出、把新程式換上去、再自動重開。
$env:PSModulePath = [IO.Path]::Combine($PSHOME, 'Modules')
$cacheRoot = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'Kuidao\DevUpdate')).TrimEnd('\')
$workRoot = [IO.Path]::GetFullPath($WorkPath).TrimEnd('\')
$target = [IO.Path]::GetFullPath($TargetExe)
$targetDir = Split-Path $target -Parent
$new = Join-Path $workRoot 'update.exe'
$backup = Join-Path $workRoot 'previous.exe'
$staged = Join-Path $targetDir ('kuidao-dev-update-' + [Guid]::NewGuid().ToString('N') + '.exe')
$replaced = $false
$exited = $false
try {
    if (-not $workRoot.StartsWith($cacheRoot + '\', [StringComparison]::OrdinalIgnoreCase)) { throw '更新快取目錄無效。' }
    if (-not (Test-Path -LiteralPath $new -PathType Leaf)) { throw '找不到新的開發版程式。' }
    if (-not (Test-Path -LiteralPath $target -PathType Leaf)) { throw '找不到目前程式。' }
    [IO.File]::Copy($new, $staged, $true)
    Set-Content -LiteralPath (Join-Path $workRoot 'ready') -Value 'ok'
    if ($ParentProcessId -gt 0) {
        $parent = Get-Process -Id $ParentProcessId -ErrorAction SilentlyContinue
        if ($parent -and -not $parent.WaitForExit(60000)) { throw '鬼島尚未退出，請稍後再試。' }
    }
    $exited = $true
    [IO.File]::Copy($target, $backup, $true)
    [IO.File]::Replace($staged, $target, [NullString]::Value)
    $replaced = $true
    Start-Process -FilePath $target -WorkingDirectory $targetDir -WindowStyle Hidden
    if ($workRoot.StartsWith($cacheRoot + '\', [StringComparison]::OrdinalIgnoreCase)) {
        try { Remove-Item -LiteralPath $workRoot -Recurse -Force } catch { }
    }
} catch {
    if ($replaced -and (Test-Path -LiteralPath $backup)) { [IO.File]::Copy($backup, $target, $true) }
    if (Test-Path -LiteralPath $staged) { Remove-Item -LiteralPath $staged -Force -ErrorAction SilentlyContinue }
    if (-not $exited) {
        [IO.File]::WriteAllText((Join-Path $workRoot 'error.txt'), $_.Exception.Message)
        exit 1
    }
    Start-Process -FilePath $target -WorkingDirectory $targetDir -WindowStyle Hidden
    exit 1
}
