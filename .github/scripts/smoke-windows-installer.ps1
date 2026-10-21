# Smoke test of a Windows installer (release workflow): silent install into a
# temporary folder, the installed app starts and stays up for 15 s on a fresh
# FREEKERF_HOME, then a silent uninstall removes it.
param(
    [Parameter(Mandatory)] [string] $Installer,
    [Parameter(Mandatory)] [ValidateSet('msi', 'nsis')] [string] $Kind
)
$ErrorActionPreference = 'Stop'

$root = Join-Path $env:RUNNER_TEMP "freekerf-$Kind"
$dir = Join-Path $root 'app'
$env:FREEKERF_HOME = Join-Path $root 'home'
New-Item -ItemType Directory -Force $env:FREEKERF_HOME | Out-Null

function Invoke-Checked([string] $file, [string[]] $arguments) {
    $p = Start-Process $file -ArgumentList $arguments -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw "$file $arguments exited with $($p.ExitCode)" }
}

Write-Host "install $Installer into $dir"
if ($Kind -eq 'msi') {
    Invoke-Checked msiexec.exe @('/i', "`"$Installer`"", '/qn', '/norestart', "INSTALLDIR=`"$dir`"", '/l*v', "`"$root\install.log`"")
} else {
    # /D must be the last argument, unquoted
    Invoke-Checked $Installer @('/S', "/D=$dir")
}

$app = Get-ChildItem $dir -Filter *.exe -Recurse | Where-Object Name -notlike 'uninstall*' | Select-Object -First 1
if (-not $app) { Get-ChildItem $root -Recurse | Format-Table FullName; throw "no app executable under $dir" }
Write-Host "start $($app.FullName)"
$p = Start-Process $app.FullName -PassThru
Start-Sleep -Seconds 15
if ($p.HasExited) { throw "the app exited with $($p.ExitCode) (it should still be running after 15 s)" }
Stop-Process -Id $p.Id -Force
$p.WaitForExit()
Write-Host 'the app stayed up for 15 s'

Write-Host 'uninstall'
if ($Kind -eq 'msi') {
    Invoke-Checked msiexec.exe @('/x', "`"$Installer`"", '/qn', '/norestart')
} else {
    Invoke-Checked (Join-Path $dir 'uninstall.exe') @('/S')
}
# the NSIS uninstaller copies itself and returns before the files are gone
for ($i = 0; $i -lt 30 -and (Test-Path $app.FullName); $i++) { Start-Sleep -Seconds 1 }
if (Test-Path $app.FullName) { throw "$($app.FullName) is still there after the uninstall" }
Write-Host "$Kind installer: OK"
