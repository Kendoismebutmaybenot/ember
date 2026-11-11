# Builds Wisp Nightly and installs it beside stable Wisp.
# Nightly uses %LOCALAPPDATA%\Programs\Wisp Nightly and %APPDATA%\wisp-nightly,
# so it never touches the stable install or its config.
param([switch]$NoBuild, [switch]$NoLaunch)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$target = Join-Path $repo 'target\nightly'
$dir = Join-Path $env:LOCALAPPDATA 'Programs\Wisp Nightly'
$data = Join-Path $env:APPDATA 'wisp-nightly'

if (-not $NoBuild) {
    cargo build --release --workspace --features 'wisp/nightly,wisp-settings/nightly' `
        --manifest-path (Join-Path $repo 'Cargo.toml') --target-dir $target
    if ($LASTEXITCODE) { throw "cargo build failed ($LASTEXITCODE)" }
}

$exe = Join-Path $dir 'ember.exe'
if (Test-Path $exe) {
    & $exe --quit
    Start-Sleep -Seconds 2
}
New-Item -ItemType Directory -Force $dir | Out-Null
foreach ($file in 'ember.exe', 'ember-settings.exe') {
    Copy-Item (Join-Path $target "release\$file") (Join-Path $dir $file) -Force
}

# First install only: start from the stable config and history, on a shortcut
# that does not clash with stable's, with the dock features turned on.
New-Item -ItemType Directory -Force $data | Out-Null
$config = Join-Path $data 'config.toml'
if (-not (Test-Path $config)) {
    $stable = Join-Path $env:APPDATA 'wisp'
    $text = if (Test-Path "$stable\config.toml") { Get-Content "$stable\config.toml" -Raw } else { '' }
    $text = $text -replace '(?m)^hotkey\s*=.*$', ''
    $text = $text -replace '(?m)^(position|dock|hide_taskbar)\s*=.*$', ''
    $text = "hotkey = `"Ctrl+Shift+Space`"`n" + $text.TrimEnd() + @"


# Nightly features
position = "bottom"
dock = true
hide_taskbar = true
"@
    Set-Content $config $text -Encoding utf8
    if (Test-Path "$stable\history.toml") { Copy-Item "$stable\history.toml" $data }
}

$shell = New-Object -ComObject WScript.Shell
$link = $shell.CreateShortcut((Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Wisp Nightly.lnk'))
$link.TargetPath = $exe
$link.WorkingDirectory = $dir
$link.Description = 'Wisp Nightly'
$link.Save()

if (-not $NoLaunch) { Start-Process $exe }
"Installed Wisp Nightly to $dir (config: $config)"
