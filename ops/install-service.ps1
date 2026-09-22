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

.EXAMPLE
    .\install-service.ps1 -InstallPath "C:\GIAMS"
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string] $InstallPath,

    [string] $ServiceName = 'GIAMS',

    [string] $DisplayName = 'GIAMS Accounting Server'
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
# `binPath` needs the working directory to be the install folder so the
# executable finds .env, the node signing key, and the tls folder beside it.

Write-Host "Installing $DisplayName…"

& sc.exe create $ServiceName `
    binPath= "`"$exe`"" `
    DisplayName= "`"$DisplayName`"" `
    start= auto | Out-Null

if ($LASTEXITCODE -ne 0) { throw "sc.exe create failed with exit code $LASTEXITCODE." }

& sc.exe description $ServiceName `
    "Holds the official accounting records for this municipality. Stopping this service takes the accounting system offline for every workstation." | Out-Null

# Restart after a crash: 60s, 60s, then every 2 minutes. The counter resets
# after a day, so a single bad afternoon does not exhaust the retries.
& sc.exe failure $ServiceName reset= 86400 actions= restart/60000/restart/60000/restart/120000 | Out-Null

# The working directory is not settable through sc.exe, so it is set on the
# service's registry key directly. Without it the service starts in
# System32 and cannot find its own .env.
$key = "HKLM:\SYSTEM\CurrentControlSet\Services\$ServiceName"
New-ItemProperty -Path $key -Name 'AppDirectory' -Value $InstallPath -PropertyType String -Force | Out-Null

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
    Write-Host 'The service did not start. Check the Windows event log:' -ForegroundColor Red
    Write-Host '    Get-EventLog -LogName Application -Newest 20 | Format-List'
    Write-Host ''
    Write-Host 'The usual causes are a wrong DATABASE_URL, PostgreSQL not running,'
    Write-Host 'or a value with a space in it left unquoted in .env.'
    exit 1
}

Write-Host 'Next:' -ForegroundColor Cyan
Write-Host '  * Confirm it answers:   curl http://localhost:8787/api/health'
Write-Host '  * Read the first-run administrator password out of the service log.'
Write-Host '  * Set up a daily backup:'
Write-Host "        schtasks /create /tn ""GIAMS backup"" /tr ""$exe backup"" /sc daily /st 18:00 /ru SYSTEM"
Write-Host '  * Before letting workstations connect over the network, run'
Write-Host "        $exe make-cert <this server's IP>"
Write-Host '    and set GIAMS_TLS_CERT and GIAMS_TLS_KEY in .env.'
Write-Host ''
