param(
    [switch]$Force,
    [switch]$Build
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$Root = Split-Path -Parent $PSScriptRoot
$DestDir = if ($env:HEADROOM_HOME) { $env:HEADROOM_HOME } else { Join-Path $env:USERPROFILE ".claude\headroom" }
$Dest = Join-Path $DestDir "headroom.exe"
$Repo = "mishoshup/headroom"
$Refresh = if ($env:HEADROOM_REFRESH) { $env:HEADROOM_REFRESH } else { "60" }

if ($Refresh -notmatch '^\d+$' -or [int]$Refresh -lt 1) {
    throw "headroom: HEADROOM_REFRESH must be a positive whole number of seconds"
}
$env:HEADROOM_REFRESH = $Refresh

function Install-Binary($src, $dest) {
    $tmp = "$dest.new"
    Copy-Item $src $tmp -Force
    for ($i = 0; $i -lt 20; $i++) {
        try {
            if (Test-Path $dest) { Move-Item $dest "$dest.old" -Force -ErrorAction Stop }
            Move-Item $tmp $dest -Force -ErrorAction Stop
            break
        } catch {
            Start-Sleep -Milliseconds 250
        }
    }
    if (-not (Test-Path $dest)) { throw "headroom: could not replace $dest" }
    Remove-Item "$dest.old" -Force -ErrorAction SilentlyContinue
}

function Build-Source {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { return $false }
    & cargo build --release --manifest-path (Join-Path $Root "Cargo.toml")
    if ($LASTEXITCODE -ne 0) { return $false }
    Install-Binary (Join-Path $Root "target\release\headroom.exe") (Join-Path $Root "bin\headroom.exe")
    return $true
}

function Download-Prebuilt {
    $target = "x86_64-pc-windows-msvc"
    $asset = "headroom-$target.zip"
    $url = "https://github.com/$Repo/releases/latest/download/$asset"
    $tmp = Join-Path $env:TEMP ("headroom-" + [guid]::NewGuid())
    New-Item -ItemType Directory -Force $tmp | Out-Null
    try {
        Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile (Join-Path $tmp $asset)
        Invoke-WebRequest -UseBasicParsing -Uri "$url.sha256" -OutFile (Join-Path $tmp "$asset.sha256")
        $expected = ((Get-Content (Join-Path $tmp "$asset.sha256") -Raw).Trim() -split '\s+')[0].ToLower()
        $actual = (Get-FileHash (Join-Path $tmp $asset) -Algorithm SHA256).Hash.ToLower()
        if ($expected -ne $actual) { throw "checksum mismatch" }
        Expand-Archive -Path (Join-Path $tmp $asset) -DestinationPath $tmp -Force
        Install-Binary (Join-Path $tmp "headroom.exe") (Join-Path $Root "bin\headroom.exe")
        Write-Host "headroom: downloaded release asset $asset"
        return $true
    } catch {
        return $false
    } finally {
        Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
    }
}

New-Item -ItemType Directory -Force -Path $DestDir | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $Root "bin") | Out-Null

if ($env:HEADROOM_BINARY -and (Test-Path $env:HEADROOM_BINARY)) {
    Install-Binary $env:HEADROOM_BINARY (Join-Path $Root "bin\headroom.exe")
} elseif ($Build) {
    if (-not (Build-Source)) { throw "headroom: -Build requires the Rust toolchain" }
} else {
    if (-not (Download-Prebuilt)) {
        Write-Host "headroom: prebuilt download failed; trying a source build"
        if (-not (Build-Source)) {
            throw "headroom: could not download a release or build from source. Install Rust or set HEADROOM_BINARY."
        }
    }
}

Install-Binary (Join-Path $Root "bin\headroom.exe") $Dest
& $Dest self-test | Out-Null
if ($LASTEXITCODE -ne 0) { throw "headroom: self-test failed" }

if ($Force) {
    & $Dest setup --force
} else {
    & $Dest setup
}
if ($LASTEXITCODE -ne 0) { throw "headroom: setup failed" }

Write-Host "headroom: installed $Dest"
Write-Host "headroom: borrow panel:  $Dest"
Write-Host "headroom: borrow CLI:    $Dest borrow 20"
Write-Host "headroom: restart Claude Code to load the hooks and statusline"
