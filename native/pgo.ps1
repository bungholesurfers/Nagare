# Builds Nagare with profile-guided optimisation: an instrumented build runs the scene in --test mode through
# the paths a session takes (both glare modes, every tone curve, auto exposure, the noise, the day cycle, the perf
# overlay; OpenGL, and Vulkan with spline.elf on the GPU and on the CPU), and the release build is then optimised
# with what those runs recorded. The result is the usual target\release\nagare.exe.
#
#   .\pgo.ps1               # train and build
#   .\pgo.ps1 -Seconds 60   # longer training runs (default 20 scene seconds each)
#   .\pgo.ps1 -Save         # train and build, and keep the profile in pgo\ for the release workflow to build from
#   .\pgo.ps1 -Use          # build from the profile kept in pgo\, no training (what the release workflow runs)
#
# The training draws the scene, so it needs a GPU: it is run on a PC, and -Save keeps what it recorded in
# pgo\nagare.profdata, beside pgo\nagare.profdata.txt naming the compiler it was recorded with. A profile only fits
# the compiler that recorded it (rust-toolchain.toml pins it) and the code it was recorded on: code changed since
# merely goes unoptimised, so -Save again before a release whose code has moved much.
#
# Needs rustup's llvm-tools (rustup component add llvm-tools). Close a running Nagare first: its exe is locked.

param([int]$Seconds = 20, [switch]$Save, [switch]$Use)
# Native tools write their progress to stderr, which 'Stop' would take for failure; exit codes are checked instead.
$ErrorActionPreference = 'Continue'
Set-Location $PSScriptRoot

$kept = Join-Path $PSScriptRoot 'pgo\nagare.profdata'
$keptNote = Join-Path $PSScriptRoot 'pgo\nagare.profdata.txt'
$compiler = (rustc -V).Trim()

# A release takes every last percent: fat LTO over one unit, where everyday builds take thin LTO over 16.
$env:CARGO_PROFILE_RELEASE_LTO = "fat"
$env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS = "1"

if ($Use) {
    if (-not (Test-Path $kept)) { throw "no profile at $kept; run .\pgo.ps1 -Save on a PC first" }
    $recorded = if (Test-Path $keptNote) { (Get-Content $keptNote -TotalCount 1).Trim() } else { 'unknown' }
    if ($recorded -ne $compiler) {
        Write-Warning "the profile was recorded with '$recorded', this build is '$compiler': it may not apply"
    }
    $merged = $kept
} else {
    $sysroot = (rustc --print sysroot).Trim()
    $host_triple = ((rustc -vV) -match '^host:' -replace 'host: ', '').Trim()
    $profdata = Join-Path $sysroot "lib\rustlib\$host_triple\bin\llvm-profdata.exe"
    if (-not (Test-Path $profdata)) {
        throw "llvm-profdata not found; run: rustup component add llvm-tools"
    }

    $profiles = Join-Path $PSScriptRoot 'target\pgo-profiles'
    $merged = Join-Path $PSScriptRoot 'target\pgo-merged.profdata'
    if (Test-Path $profiles) { Remove-Item -Recurse -Force $profiles }
    New-Item -ItemType Directory -Force $profiles | Out-Null

    Write-Host '== instrumented build'
    $env:RUSTFLAGS = "-Cprofile-generate=$profiles"
    cargo build --release --target-dir target\pgo-instrumented
    if ($LASTEXITCODE -ne 0) { throw 'instrumented build failed' }
    $exe = 'target\pgo-instrumented\release\nagare.exe'

    # The training: what the scene draws by default, then each enhancement's path and the console's own glare.
    $runs = @(
        @(),
        @('--set', 'glareMode=0'),
        @('--set', 'glareMode=0', '--set', 'glareRadiusScaleOn=1'),
        @('--set', 'tonemap=1', '--set', 'aeEnabled=1'),
        @('--set', 'tonemap=2', '--set', 'noiseEnabled=1'),
        @('--set', 'tonemap=3', '--set', 'desatEnabled=1'),
        @('--set', 'tonemap=4', '--perf-overlay', '3'),
        @('--set', 'tonemap=5', '--test-date', '2026-01-10T07:00:00'),
        @('--test-date', '2026-09-01T13:00:00', '--set', 'aeEnabled=1', '--set', 'aeMetering=1'),
        @('--renderer', 'vulkan'),
        @('--renderer', 'vulkan', '--set', 'gpuWave=0', '--set', 'glareMode=0', '--set', 'aeEnabled=1'),
        @('--renderer', 'vulkan', '--set', 'tonemap=3', '--perf-overlay', '3', '--ui')
    )
    foreach ($extra in $runs) {
        Write-Host "== training: $($extra -join ' ')"
        & $exe --test --exit-after $Seconds @extra | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "training run failed: $($extra -join ' ')" }
    }

    Write-Host '== merging the profiles'
    & $profdata merge -o $merged $profiles
    if ($LASTEXITCODE -ne 0) { throw 'llvm-profdata merge failed' }

    if ($Save) {
        New-Item -ItemType Directory -Force (Split-Path $kept) | Out-Null
        Copy-Item $merged $kept -Force
        $compiler | Set-Content $keptNote -Encoding ascii
        Write-Host "kept the profile: $kept (recorded with $compiler)"
    }
}

Write-Host '== optimised build'
$env:RUSTFLAGS = "-Cprofile-use=$merged"
cargo build --release
$status = $LASTEXITCODE
Remove-Item Env:\RUSTFLAGS
if ($status -ne 0) { throw 'optimised build failed' }
Write-Host 'done: target\release\nagare.exe'
