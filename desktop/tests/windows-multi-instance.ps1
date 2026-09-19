param([Parameter(Mandatory=$true)][string]$Binary)
$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path -LiteralPath $Binary).Path
$scratch = Join-Path ([IO.Path]::GetTempPath()) ('medsci-gui-' + [Guid]::NewGuid().ToString('N'))
$same = Join-Path $scratch '同一个目录 with spaces'
$other = Join-Path $scratch 'other workspace'
$owned = @()
try {
    New-Item -ItemType Directory -Path $same,$other -Force | Out-Null
    foreach ($directory in @($same,$same)) {
        $owned += Start-Process -FilePath $Binary -ArgumentList ('"' + $directory + '"') -WorkingDirectory $directory -WindowStyle Hidden -PassThru
    }
    $owned += Start-Process -FilePath $Binary -WorkingDirectory $other -WindowStyle Hidden -PassThru
    $deadline = [DateTime]::UtcNow.AddSeconds(25)
    do {
        foreach ($process in $owned) { $process.Refresh() }
        if (@($owned | Where-Object HasExited).Count) { throw 'A launched GUI exited instead of opening independently' }
        if (@($owned | Where-Object { $_.MainWindowTitle -like '*Codewhale-MedSci*' }).Count -eq 3) { break }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    $directories = @($same,$same,$other)
    for ($index=0; $index -lt 3; $index++) {
        if (!$owned[$index].MainWindowTitle.Contains($directories[$index])) {
            throw ('Wrong workspace title: ' + $owned[$index].MainWindowTitle)
        }
    }
    if (@($owned.Id | Select-Object -Unique).Count -ne 3) { throw 'Expected three independent GUI processes' }
    if (!$owned[0].CloseMainWindow() -or !$owned[0].WaitForExit(10000)) { throw 'First GUI failed to close normally' }
    foreach ($process in $owned[1..2]) {
        $process.Refresh()
        if ($process.HasExited) { throw 'Closing one GUI terminated a peer' }
    }
    Write-Output 'PASS: three real GUI windows; same and different directories; independent close; no Agent/model requests'
} finally {
    foreach ($process in $owned) {
        $process.Refresh()
        if (!$process.HasExited) {
            [void]$process.CloseMainWindow()
            if (!$process.WaitForExit(10000)) { $process.Kill(); $process.WaitForExit() }
        }
    }
    $resolvedScratch = [IO.Path]::GetFullPath($scratch)
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if (!$resolvedScratch.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe cleanup path' }
    if (Test-Path -LiteralPath $resolvedScratch) { Remove-Item -LiteralPath $resolvedScratch -Recurse -Force }
}
