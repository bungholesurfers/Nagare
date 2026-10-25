# Builds Nagare's Windows installer: the release exes (a plain build; run pgo.ps1 first for a release, then pass
# -NoBuild), then installer\nagare.iss through Inno Setup 6's compiler, into target\installer\nagare-setup-VERSION.exe.
# With -Install it then installs it, silently, over the installed copy. installer\INSTALLER.md has the whole story.
#
#   .\installer.ps1                    # build and package
#   .\installer.ps1 -NoBuild           # package the exes already in target\release
#   .\installer.ps1 -Install           # build, package and install
#
# Needs Inno Setup 6 (winget install JRSoftware.InnoSetup). A running Nagare is stopped by the installer, not here;
# the build itself needs target\release\nagare.exe free, so a Nagare run from there is stopped first.

param([switch]$NoBuild, [switch]$Install)
$ErrorActionPreference = 'Continue'
Set-Location $PSScriptRoot

if (-not $NoBuild) {
    # The exe being replaced cannot be running: stop the build folder's own Nagare, and its watchdog.
    $release = (Resolve-Path target\release -ErrorAction SilentlyContinue).Path
    if ($release) {
        Get-Process nagare, nagare-watchdog -ErrorAction SilentlyContinue |
            Where-Object { $_.Path -and $_.Path.StartsWith($release, [StringComparison]::OrdinalIgnoreCase) } |
            Sort-Object { $_.Name -ne 'nagare-watchdog' } |
            ForEach-Object { Write-Host "stopping $($_.Path)"; Stop-Process -Id $_.Id -Force }
    }
    cargo build --release --bin nagare --bin nagare-watchdog
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
}

$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$iscc = @(
    (Get-Command iscc -ErrorAction SilentlyContinue).Source,
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if (-not $iscc) { throw 'Inno Setup 6 not found; run: winget install JRSoftware.InnoSetup' }

& $iscc /Q "/DAppVersion=$version" installer\nagare.iss
if ($LASTEXITCODE -ne 0) { throw 'the installer did not compile' }
$setup = Resolve-Path "target\installer\nagare-setup-$version.exe"
Write-Host "built $setup"

if ($Install) {
    # No wizard and no questions; the tasks are those chosen last time (Start with Windows, ticked to begin with), and
    # a Nagare the installer stopped is started again.
    $log = Join-Path $env:TEMP 'nagare-setup.log'
    Write-Host "installing (log: $log)"
    $p = Start-Process -FilePath $setup -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/LOG=`"$log`"" -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw "the install failed (exit code $($p.ExitCode)); see $log" }
    Write-Host "installed into $env:LOCALAPPDATA\Programs\Nagare"
}
