#Requires -Version 5.1
param(
    [switch]$Force
)
$ErrorActionPreference = 'Stop'

$Repo = "Dmitrytuk/cretopass"
$Bin  = "cretopass.exe"

# Base URL for release assets.
# Honours CRETOPASS_INSTALL_URL to allow tests (and mirrors) to point at a
# different source. An empty value is treated as unset.
$BaseUrl = if ($env:CRETOPASS_INSTALL_URL) {
    $env:CRETOPASS_INSTALL_URL
} else {
    "https://github.com/$Repo/releases/latest/download"
}

# --- Architecture (WOW64-aware) ---
function Get-Arch {
    if ($env:PROCESSOR_ARCHITEW6432) { return $env:PROCESSOR_ARCHITEW6432 }
    return $env:PROCESSOR_ARCHITECTURE
}

$Arch = Get-Arch
switch ($Arch) {
    'AMD64' { $Target = 'x86_64-pc-windows-msvc' }
    'ARM64' { $Target = 'aarch64-pc-windows-msvc' }
    default { Write-Error "Unsupported architecture: $Arch"; exit 1 }
}

# --- Detect existing installation ---
$InstallDir = Join-Path $env:LOCALAPPDATA "Programs\cretopass"
$BinPath = Join-Path $InstallDir $Bin
$PrevVersion = ""
if (Test-Path $BinPath) {
    $raw = & $BinPath --version 2>$null | Select-Object -First 1
    if ($raw -match '(\d+\.\d+\.\d+\S*)') { $PrevVersion = $Matches[1] }
    if ($PrevVersion) { Write-Host "Existing installation detected: $PrevVersion" }
}

# --- Resolve latest version (via redirect, no API rate limit) ---
$LatestVersion = ""
try {
    $oldPref = $ProgressPreference
    $ProgressPreference = 'SilentlyContinue'
    $resp = Invoke-WebRequest -Uri "https://github.com/$Repo/releases/latest" `
        -UseBasicParsing -MaximumRedirection 5
    $ProgressPreference = $oldPref
    $finalUrl = $null
    if ($resp.BaseResponse.ResponseUri) {
        $finalUrl = $resp.BaseResponse.ResponseUri.AbsoluteUri
    } elseif ($resp.BaseResponse.RequestMessage.RequestUri) {
        $finalUrl = $resp.BaseResponse.RequestMessage.RequestUri.AbsoluteUri
    }
    if ($finalUrl -match '/tag/v?([^/]+)$') { $LatestVersion = $Matches[1] }
} catch {
    Write-Warning "Could not resolve latest version: $_"
}

# --- Early exit if up to date ---
if ($PrevVersion -and $LatestVersion -and `
    $PrevVersion -eq $LatestVersion -and -not $Force) {
    Write-Host "Already up to date: $PrevVersion"
    Write-Host "Use -Force to reinstall."
    exit 0
}

# --- URL ---
$ZipName = "cretopass-$Target.zip"
$Url     = "$BaseUrl/$ZipName"
$ShaUrl  = "$Url.sha256"

# --- Download + verify + install (with temp cleanup) ---
$Tmp = Join-Path $env:TEMP "cretopass-$PID"
New-Item -ItemType Directory -Path $Tmp -Force | Out-Null
try {
    Invoke-WebRequest -Uri $Url -OutFile (Join-Path $Tmp $ZipName) -UseBasicParsing

    # --- Checksum (fail-soft with prompt) ---
    try {
        Invoke-WebRequest -Uri $ShaUrl -OutFile (Join-Path $Tmp "$ZipName.sha256") `
            -UseBasicParsing -ErrorAction Stop
        $Expected = (Get-Content (Join-Path $Tmp "$ZipName.sha256") -Raw).Split()[0].ToLower()
        $Actual   = (Get-FileHash (Join-Path $Tmp $ZipName) -Algorithm SHA256).Hash.ToLower()
        if ($Expected -ne $Actual) {
            Write-Warning "Checksum mismatch!"
            if (-not [Environment]::UserInteractive) {
                Write-Error "Non-interactive mode: cannot prompt. Aborting."
                exit 1
            }
            $ans = Read-Host "Continue anyway? [y/N]"
            if ($ans -notmatch '^[yY]') { Write-Host "Aborted."; exit 1 }
        }
    } catch {
        Write-Warning "Checksum file not found."
        if (-not [Environment]::UserInteractive) {
            Write-Error "Non-interactive mode: cannot prompt. Aborting."
            exit 1
        }
        $ans = Read-Host "Continue without verification? [y/N]"
        if ($ans -notmatch '^[yY]') { Write-Host "Aborted."; exit 1 }
    }

    # --- Verify archive integrity ---
    try {
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $zip = [System.IO.Compression.ZipFile]::OpenRead((Join-Path $Tmp $ZipName))
        $zip.Dispose()
    } catch {
        Write-Error "Downloaded archive is corrupted: $_"
        exit 1
    }

    # --- Install ---
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Expand-Archive -Path (Join-Path $Tmp $ZipName) -DestinationPath $InstallDir -Force
} finally {
    Remove-Item $Tmp -Recurse -Force -ErrorAction SilentlyContinue
}

# --- Start Menu shortcut (launches via cmd.exe /k) ---
function Install-StartMenuShortcut {
    $IconPath = Join-Path $InstallDir "cretopass.ico"
    try {
        Invoke-WebRequest `
            -Uri "$BaseUrl/cretopass.ico" `
            -OutFile $IconPath -UseBasicParsing -ErrorAction Stop
    } catch {
        Write-Warning "Icon not installed: $_"
    }

    $StartMenu = [Environment]::GetFolderPath('Programs')
    $Lnk = Join-Path $StartMenu "cretopass.lnk"

    $Wsh = New-Object -ComObject WScript.Shell
    $Sc = $Wsh.CreateShortcut($Lnk)
    $Sc.TargetPath = "$env:SystemRoot\System32\cmd.exe"
    $Sc.Arguments = "/k `"$BinPath`""
    $Sc.WorkingDirectory = $InstallDir
    if (Test-Path $IconPath) { $Sc.IconLocation = "$IconPath,0" }
    $Sc.Description = "cretopass"
    $Sc.Save()
}
Install-StartMenuShortcut

# --- PATH (SetEnvironmentVariable, no sudo) ---
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
$paths = @()
if ($UserPath) { $paths = $UserPath -split ';' | Where-Object { $_ } }
if ($paths -notcontains $InstallDir) {
    $NewPath = if ([string]::IsNullOrEmpty($UserPath)) { $InstallDir } else { "$UserPath;$InstallDir" }
    [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")
    Write-Host "Added $InstallDir to user PATH."
    Write-Host "Restart your terminal for changes to take effect."
}

# --- PowerShell completions ---
function Install-PowerShellCompletion {
    $CompDir = Join-Path (Join-Path $env:USERPROFILE "Documents") "PowerShell\Completions"
    New-Item -ItemType Directory -Path $CompDir -Force | Out-Null

    $CompPath = Join-Path $CompDir "cretopass.ps1"
    try {
        Invoke-WebRequest `
            -Uri "$BaseUrl/cretopass.ps1" `
            -OutFile $CompPath -UseBasicParsing -ErrorAction Stop
    } catch {
        Write-Warning "PowerShell completion not installed: $_"
        return
    }

    # Add dot-source line to PowerShell profile (dedup check)
    $ProfilePath = $PROFILE.CurrentUserAllHosts
    if (-not $ProfilePath) { return }

    $ProfileDir = Split-Path $ProfilePath -Parent
    if (-not (Test-Path $ProfileDir)) {
        New-Item -ItemType Directory -Path $ProfileDir -Force | Out-Null
    }

    $dotLine = ". `"$CompPath`""
    if (Test-Path $ProfilePath) {
        $existing = Get-Content $ProfilePath -Raw
        if ($existing -notmatch [regex]::Escape($dotLine)) {
            Add-Content $ProfilePath "`n$dotLine"
        }
    } else {
        Set-Content $ProfilePath $dotLine
    }
}
Install-PowerShellCompletion

# --- Report ---
$NewVersion = ""
if (Test-Path $BinPath) {
    $raw = & $BinPath --version 2>$null | Select-Object -First 1
    if ($raw -match '(\d+\.\d+\.\d+\S*)') { $NewVersion = $Matches[1] }
}
$shown = if ($NewVersion) { $NewVersion } else { 'unknown' }
if ($PrevVersion) {
    Write-Host "Installed: $PrevVersion -> $shown"
} else {
    Write-Host "Installed: <none> -> $shown"
}

Write-Host "Installed $Bin to $BinPath"