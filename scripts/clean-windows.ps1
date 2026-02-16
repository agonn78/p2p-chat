[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$ScriptArgs
)

$ErrorActionPreference = 'Stop'

function Show-Usage {
    @'
Usage:
  clean-windows.ps1 [--dry-run] [--clean] [--reinstall] [--dev|--build] [--purge-app-data]

Modes:
  --dry-run         Show what would be deleted (default)
  --clean           Delete caches for real
  --reinstall       Run npm ci after cleaning
  --dev             Run npm run tauri dev after cleaning
  --build           Run npm run tauri build after cleaning
  --purge-app-data  Also delete app data/cache directories for this Tauri app

Examples:
  .\scripts\clean-windows.ps1 --dry-run
  .\scripts\clean-windows.ps1 --clean
  .\scripts\clean-windows.ps1 --clean --purge-app-data
  .\scripts\clean-windows.ps1 --clean --reinstall --dev
'@
}

$DryRun = $true
$DoReinstall = $false
$DoDev = $false
$DoBuild = $false
$PurgeAppData = $false

foreach ($arg in $ScriptArgs) {
    switch ($arg) {
        '--dry-run' { $DryRun = $true }
        '--clean' { $DryRun = $false }
        '--reinstall' { $DoReinstall = $true }
        '--dev' { $DoDev = $true }
        '--build' { $DoBuild = $true }
        '--purge-app-data' { $PurgeAppData = $true }
        '-h' { Show-Usage; exit 0 }
        '--help' { Show-Usage; exit 0 }
        default {
            Write-Error "Unknown option: $arg"
            Show-Usage
            exit 1
        }
    }
}

if ($DoDev -and $DoBuild) {
    throw 'Error: --dev and --build are mutually exclusive.'
}

if ($DryRun -and ($DoReinstall -or $DoDev -or $DoBuild)) {
    throw 'Error: --reinstall/--dev/--build require --clean.'
}

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = (Resolve-Path (Join-Path $ScriptDir '..')).Path
$DesktopDir = Join-Path $ProjectRoot 'apps\desktop'
$TauriConf = Join-Path $DesktopDir 'src-tauri\tauri.conf.json'

if (-not (Test-Path -LiteralPath $DesktopDir -PathType Container)) {
    throw "Expected desktop app at $DesktopDir"
}

$removed = New-Object System.Collections.Generic.List[object]

$normalizedProjectRoot = [System.IO.Path]::GetFullPath($ProjectRoot)
$allowedAppRoots = @()
if ($env:APPDATA) {
    $allowedAppRoots += [System.IO.Path]::GetFullPath($env:APPDATA)
}
if ($env:LOCALAPPDATA) {
    $allowedAppRoots += [System.IO.Path]::GetFullPath($env:LOCALAPPDATA)
}

function Test-SafeProjectPath {
    param([Parameter(Mandatory = $true)][string]$Path)
    $normalized = [System.IO.Path]::GetFullPath($Path)
    return $normalized.StartsWith($normalizedProjectRoot, [System.StringComparison]::OrdinalIgnoreCase)
}

function Test-SafeAppDataPath {
    param([Parameter(Mandatory = $true)][string]$Path)
    $normalized = [System.IO.Path]::GetFullPath($Path)
    foreach ($root in $allowedAppRoots) {
        if ($normalized.StartsWith($root, [System.StringComparison]::OrdinalIgnoreCase)) {
            return $true
        }
    }
    return $false
}

function Remove-PathSafe {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Reason,
        [Parameter(Mandatory = $true)][ValidateSet('Project', 'AppData')][string]$Scope
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        return
    }

    $normalized = [System.IO.Path]::GetFullPath($Path)

    if ($Scope -eq 'Project') {
        if (-not (Test-SafeProjectPath -Path $normalized)) {
            Write-Host "[skip] unsafe project path: $normalized"
            return
        }
    }
    else {
        if (-not (Test-SafeAppDataPath -Path $normalized)) {
            Write-Host "[skip] unsafe app data path: $normalized"
            return
        }
    }

    if ($DryRun) {
        Write-Host "[dry-run] would remove: $normalized ($Reason)"
    }
    else {
        Write-Host "[clean] removing: $normalized ($Reason)"
        Remove-Item -LiteralPath $normalized -Recurse -Force
    }

    $removed.Add([PSCustomObject]@{
            Path = $normalized
            Reason = $Reason
        })
}

Write-Host "Project root: $ProjectRoot"
if ($DryRun) {
    Write-Host 'Mode: dry-run'
}
else {
    Write-Host 'Mode: clean'
}

$projectTargets = @(
    @{ Path = (Join-Path $DesktopDir 'node_modules'); Reason = 'Node dependencies' },
    @{ Path = (Join-Path $DesktopDir '.vite'); Reason = 'Vite cache' },
    @{ Path = (Join-Path $DesktopDir 'node_modules\.vite'); Reason = 'Vite pre-bundle cache' },
    @{ Path = (Join-Path $DesktopDir 'dist'); Reason = 'Frontend build output' },
    @{ Path = (Join-Path $DesktopDir 'build'); Reason = 'Alternate build output' },
    @{ Path = (Join-Path $DesktopDir '.turbo'); Reason = 'Turbo cache' },
    @{ Path = (Join-Path $DesktopDir '.parcel-cache'); Reason = 'Parcel cache' },
    @{ Path = (Join-Path $DesktopDir '.cache'); Reason = 'Generic local cache' },
    @{ Path = (Join-Path $DesktopDir '.eslintcache'); Reason = 'ESLint cache' },
    @{ Path = (Join-Path $DesktopDir 'src-tauri\target'); Reason = 'Tauri target artifacts' },
    @{ Path = (Join-Path $ProjectRoot 'target'); Reason = 'Workspace target artifacts' }
)

foreach ($target in $projectTargets) {
    Remove-PathSafe -Path $target.Path -Reason $target.Reason -Scope Project
}

if ($PurgeAppData) {
    if (-not (Test-Path -LiteralPath $TauriConf -PathType Leaf)) {
        throw "Missing Tauri config at $TauriConf"
    }

    $tauri = Get-Content -LiteralPath $TauriConf -Raw | ConvertFrom-Json
    $appIdentifier = [string]$tauri.identifier
    $productName = [string]$tauri.productName

    if ([string]::IsNullOrWhiteSpace($appIdentifier) -or [string]::IsNullOrWhiteSpace($productName)) {
        throw "Could not read identifier/productName from $TauriConf"
    }

    Write-Host "Detected app identifier: $appIdentifier"
    Write-Host "Detected product name: $productName"

    $appNames = @($appIdentifier.Trim(), $productName.Trim()) | Sort-Object -Unique
    foreach ($appName in $appNames) {
        if ($env:APPDATA) {
            Remove-PathSafe -Path (Join-Path $env:APPDATA $appName) -Reason 'App data (roaming)' -Scope AppData
        }
        if ($env:LOCALAPPDATA) {
            Remove-PathSafe -Path (Join-Path $env:LOCALAPPDATA $appName) -Reason 'App data/cache (local)' -Scope AppData
            Remove-PathSafe -Path (Join-Path $env:LOCALAPPDATA "$appName.WebView2") -Reason 'WebView2 cache' -Scope AppData
        }
    }
}

if ($removed.Count -eq 0) {
    Write-Host 'No cache paths found to clean.'
}
else {
    Write-Host ''
    Write-Host "Affected paths ($($removed.Count)):"
    foreach ($entry in $removed) {
        Write-Host "- $($entry.Path) [$($entry.Reason)]"
    }
}

if ($DryRun) {
    Write-Host ''
    Write-Host 'Dry-run complete. Re-run with --clean to apply deletion.'
    exit 0
}

if ($DoReinstall -or $DoDev -or $DoBuild) {
    if (-not (Get-Command npm -ErrorAction SilentlyContinue)) {
        throw 'npm is required for --reinstall/--dev/--build'
    }
}

Push-Location $DesktopDir
try {
    if ($DoReinstall) {
        Write-Host ''
        Write-Host '[step] npm ci'
        npm ci
        if ($LASTEXITCODE -ne 0) {
            throw 'npm ci failed'
        }
    }

    if ($DoDev) {
        Write-Host ''
        Write-Host '[step] npm run tauri dev'
        npm run tauri dev
        if ($LASTEXITCODE -ne 0) {
            throw 'npm run tauri dev failed'
        }
    }

    if ($DoBuild) {
        Write-Host ''
        Write-Host '[step] npm run tauri build'
        npm run tauri build
        if ($LASTEXITCODE -ne 0) {
            throw 'npm run tauri build failed'
        }
    }
}
finally {
    Pop-Location
}

Write-Host ''
Write-Host 'Clean workflow completed successfully.'
