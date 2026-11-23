<#
.SYNOPSIS
    Installs the GIAMS accounting server as a Windows Service.

.DESCRIPTION
    Run once, as Administrator, on the Accounting Office server.

    A service rather than a logged-in program, because the alternative is a
    console window somebody eventually closes — and when it closes, the whole
    office loses the system until someone notices and starts it again.

    The service is set to restart itself on failure, so a crash at four in the
    afternoon does not become an outage until the next morning.

.PARAMETER InstallPath
    Folder holding giams-node.exe and its .env file.

.PARAMETER ServiceName
    Windows service name. Leave as GIAMS unless you have a reason.

.PARAMETER SourcePath
    Optional. A GIAMS project or release folder to copy from: giams-node.exe
    (target\release\ in a project), .env, node-signing.key, tls\ and, where
    .env names one, the licensing key. Without it, InstallPath must already
    hold them.

.EXAMPLE
    .\install-service.ps1 -InstallPath "C:\GIAMS"

.EXAMPLE
    .\install-service.ps1 -InstallPath "C:\GIAMS" -SourcePath "D:\project\giams"
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string] $InstallPath,

    [string] $ServiceName = 'GIAMS',

    [string] $DisplayName = 'GIAMS Accounting Server',

    [string] $SourcePath
)

$ErrorActionPreference = 'Stop'

# ── Checks before touching anything ─────────────────────────────────────────
# Every one of these is a failure that would otherwise appear as a service that
# installs cleanly and then refuses to start, with the reason buried in the
# Windows event log.

$identity  = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Run this from an Administrator PowerShell. Installing a service needs it.'
}

if ($SourcePath) {
    $SourcePath = (Resolve-Path $SourcePath).Path
    New-Item -ItemType Directory -Force -Path $InstallPath | Out-Null

    # an existing service holds the exe open; stop it before replacing it
    if (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue) {
        & sc.exe stop $ServiceName | Out-Null
        Start-Sleep -Seconds 3
    }

    $built = @('target\release\giams-node.exe', 'giams-node.exe') |
        ForEach-Object { Join-Path $SourcePath $_ } |
        Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $built) { throw "No giams-node.exe in $SourcePath. Build it first: cargo build --release -p giams-server" }
    Copy-Item $built (Join-Path $InstallPath 'giams-node.exe') -Force

    foreach ($item in '.env', 'node-signing.key', 'tls') {
        $from = Join-Path $SourcePath $item
        if (Test-Path $from) { Copy-Item $from $InstallPath -Recurse -Force }
    }

    # the licensing key only goes where .env asks for it, at the same relative path
    $keyLine = Get-Content (Join-Path $SourcePath '.env') | Where-Object { $_ -match '^\s*GIAMS_LICENCE_AUTHORITY_KEY\s*=' }
    if ($keyLine) {
        $keyPath = ($keyLine -split '=', 2)[1].Trim().Trim('"')
        if ($keyPath -and -not [IO.Path]::IsPathRooted($keyPath)) {
            $to = Join-Path $InstallPath $keyPath
            New-Item -ItemType Directory -Force -Path (Split-Path $to) | Out-Null
            Copy-Item (Join-Path $SourcePath $keyPath) $to -Force
        }
    }
    Write-Host "Copied the server and its settings from $SourcePath"
}

$InstallPath = (Resolve-Path $InstallPath).Path
$exe = Join-Path $InstallPath 'giams-node.exe'

if (-not (Test-Path $exe)) {
    throw "giams-node.exe is not in $InstallPath. Copy the build there first."
}

$envFile = Join-Path $InstallPath '.env'
if (-not (Test-Path $envFile)) {
    throw ".env is not in $InstallPath. Copy .env.example to .env and fill it in first."
}

# A shipped signing secret is not a secret. Refuse rather than install an
# installation that every other installation can forge sessions for.
$envText = Get-Content $envFile -Raw
if ($envText -match 'CHANGE-ME') {
    throw ".env still contains CHANGE-ME. Set a real DATABASE_URL and GIAMS_TOKEN_SECRET first."
}

if (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue) {
    Write-Host "Service '$ServiceName' already exists. Stopping and removing it first." -ForegroundColor Yellow
    & sc.exe stop $ServiceName | Out-Null
    Start-Sleep -Seconds 3
    & sc.exe delete $ServiceName | Out-Null
    Start-Sleep -Seconds 2
}

# ── Install ─────────────────────────────────────────────────────────────────
# `service` makes the program report to the service manager, work from its own
# folder (where .env, the keys and tls\ are) and log to logs\giams-node.log.

Write-Host "Installing $DisplayName…"

# start after PostgreSQL at boot, or the first attempt finds no database
$postgres = Get-Service -Name 'postgresql*' -ErrorAction SilentlyContinue | Select-Object -First 1
$depend = if ($postgres) { $postgres.Name } else { '' }

& sc.exe create $ServiceName `
    binPath= "`"$exe`" service" `
    DisplayName= "`"$DisplayName`"" `
    depend= $depend `
    start= delayed-auto | Out-Null

if ($LASTEXITCODE -ne 0) { throw "sc.exe create failed with exit code $LASTEXITCODE." }

& sc.exe description $ServiceName `
    "Holds the official accounting records for this municipality. Stopping this service takes the accounting system offline for every workstation." | Out-Null

# Restart after a crash: 60s, 60s, then every 2 minutes. The counter resets
# after a day, so a single bad afternoon does not exhaust the retries.
& sc.exe failure $ServiceName reset= 86400 actions= restart/60000/restart/60000/restart/120000 | Out-Null
# also restart when it stops with an error (database down at boot), not only on a crash
& sc.exe failureflag $ServiceName 1 | Out-Null

# ── Start ───────────────────────────────────────────────────────────────────
Write-Host 'Starting…'
& sc.exe start $ServiceName | Out-Null
Start-Sleep -Seconds 8

$service = Get-Service -Name $ServiceName
Write-Host ''
Write-Host "  Service:  $ServiceName" -ForegroundColor Green
Write-Host "  State:    $($service.Status)" -ForegroundColor Green
Write-Host "  Folder:   $InstallPath"
Write-Host ''

if ($service.Status -ne 'Running') {
    Write-Host 'The service did not start. Its log says why:' -ForegroundColor Red
    Write-Host "    Get-Content '$InstallPath\logs\giams-node.log' -Tail 30"
    Write-Host ''
    Write-Host 'The usual causes are a wrong DATABASE_URL, PostgreSQL not running,'
    Write-Host 'or a value with a space in it left unquoted in .env.'
    exit 1
}

Write-Host 'Next:' -ForegroundColor Cyan
Write-Host '  * Confirm it answers:   curl http://localhost:8787/api/health'
Write-Host "  * On a new installation, read the first-run administrator password in $InstallPath\logs\giams-node.log"
Write-Host '  * Set up a daily backup:'
Write-Host "        schtasks /create /tn ""GIAMS backup"" /tr ""$exe backup"" /sc daily /st 18:00 /ru SYSTEM"
Write-Host '  * Before letting workstations connect over the network, run'
Write-Host "        $exe make-cert <this server's IP>"
Write-Host '    and set GIAMS_TLS_CERT and GIAMS_TLS_KEY in .env.'
Write-Host ''
