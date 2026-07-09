<#
.SYNOPSIS
Coagent plugin state layer — JSON-file-based job tracking for /coagent:status queries.

.DESCRIPTION
Maintains state.json and jobs/*.json under the plugin data directory.
This is the low-latency query source for job status. The authoritative audit
trail remains in the Rust SQLite layer (coagent.sqlite).

State file layout:
  $PLUGIN_DATA/state/coagent/<workspace-slug>-<sha256>/
    state.json          # config + job index (max 50 jobs)
    jobs/<job-id>.json   # per-job detail + result payload
    jobs/<job-id>.log    # per-job progress log
#>

using namespace System.Security.Cryptography
using namespace System.Text

$script:STATE_VERSION = 1
$script:MAX_JOBS = 50

function Get-PluginDataDir {
    $envDir = $env:CODEX_PLUGIN_DATA
    if ($envDir) { return Join-Path $envDir 'state' 'coagent' }
    return Join-Path ([System.IO.Path]::GetTempPath()) 'coagent-plugin-state'
}

function Get-WorkspaceSlug {
    param([string]$WorkspaceRoot)
    $name = Split-Path $WorkspaceRoot -Leaf
    $name = $name -replace '[^a-zA-Z0-9._-]+', '-' -replace '^-+|-+$', ''
    if (-not $name) { $name = 'workspace' }
    $hash = [SHA256]::HashData([Encoding]::UTF8.GetBytes($WorkspaceRoot))
    $hex = -join ($hash[0..7] | ForEach-Object { $_.ToString('x2') })
    return $name + "-" + $hex
}

function Get-StateDir {
    param([string]$WorkspaceRoot)
    $slug = Get-WorkspaceSlug $WorkspaceRoot
    return Join-Path (Get-PluginDataDir) $slug
}

function Get-StateFile {
    param([string]$WorkspaceRoot)
    return Join-Path (Get-StateDir $WorkspaceRoot) 'state.json'
}

function Get-JobsDir {
    param([string]$WorkspaceRoot)
    return Join-Path (Get-StateDir $WorkspaceRoot) 'jobs'
}

function Get-JobFile {
    param([string]$WorkspaceRoot, [string]$JobId)
    return Join-Path (Get-JobsDir $WorkspaceRoot) ($JobId + ".json")
}

function Get-JobLogFile {
    param([string]$WorkspaceRoot, [string]$JobId)
    return Join-Path (Get-JobsDir $WorkspaceRoot) ($JobId + ".log")
}

function New-DefaultState {
    return @{ version = $script:STATE_VERSION; config = @{ stopReviewGate = $false }; jobs = @() }
}

function Read-State {
    param([string]$WorkspaceRoot)
    $file = Get-StateFile $WorkspaceRoot
    if (-not (Test-Path $file)) { return New-DefaultState }
    try {
        $parsed = Get-Content $file -Raw | ConvertFrom-Json -AsHashtable
        $default = New-DefaultState
        if (-not $parsed.config) { $parsed.config = $default.config }
        if (-not $parsed.jobs) { $parsed.jobs = $default.jobs }
        return $parsed
    } catch { return New-DefaultState }
}

function Write-State {
    param([string]$WorkspaceRoot, [hashtable]$State)
    $stateDir = Get-StateDir $WorkspaceRoot
    $jobsDir = Get-JobsDir $WorkspaceRoot
    New-Item -ItemType Directory -Force $stateDir | Out-Null
    New-Item -ItemType Directory -Force $jobsDir | Out-Null
    # Prune to MAX_JOBS
    if ($State.jobs.Count -gt $script:MAX_JOBS) {
        $State.jobs = @($State.jobs | Sort-Object { $_.updatedAt } -Descending | Select-Object -First $script:MAX_JOBS)
    }
    $json = $State | ConvertTo-Json -Depth 10 -Compress
    Set-Content -Path (Get-StateFile $WorkspaceRoot) -Value $json -Encoding utf8
}

function Get-NowIso {
    return (Get-Date).ToString('o')
}

function New-JobId {
    param([string]$Prefix = 'job')
    $random = -join ((1..6) | ForEach-Object { [char]([int][char]'a' + (Get-Random -Maximum 26)) })
    $ts = [Convert]::ToString([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds(), 16)
    return '{0}-{1}-{2}' -f $Prefix, $ts, $random
}

function Upsert-Job {
    param([string]$WorkspaceRoot, [hashtable]$JobPatch)
    $state = Read-State $WorkspaceRoot
    $now = Get-NowIso
    $existing = $state.jobs | Where-Object { $_.id -eq $JobPatch.id }
    if ($existing) {
        foreach ($key in $JobPatch.Keys) { $existing.$key = $JobPatch[$key] }
        $existing.updatedAt = $now
    } else {
        $JobPatch.createdAt = $now
        $JobPatch.updatedAt = $now
        $state.jobs = @($JobPatch) + @($state.jobs)
    }
    Write-State $WorkspaceRoot $state
}

function Read-Jobs {
    param([string]$WorkspaceRoot)
    $state = Read-State $WorkspaceRoot
    return @($state.jobs | Sort-Object { $_.updatedAt } -Descending)
}

function Read-JobFile {
    param([string]$WorkspaceRoot, [string]$JobId)
    $file = Get-JobFile $WorkspaceRoot $JobId
    if (-not (Test-Path $file)) { return $null }
    return Get-Content $file -Raw | ConvertFrom-Json -AsHashtable
}

function Write-JobFile {
    param([string]$WorkspaceRoot, [string]$JobId, [hashtable]$Payload)
    $jobsDir = Get-JobsDir $WorkspaceRoot
    New-Item -ItemType Directory -Force $jobsDir | Out-Null
    $json = $Payload | ConvertTo-Json -Depth 10 -Compress
    Set-Content -Path (Get-JobFile $WorkspaceRoot $JobId) -Value $json -Encoding utf8
}

function Add-JobLogLine {
    param([string]$WorkspaceRoot, [string]$JobId, [string]$Message)
    $file = Get-JobLogFile $WorkspaceRoot $JobId
    $line = "[" + (Get-NowIso) + "] " + $Message
    Add-Content -Path $file -Value $line -Encoding utf8
}

function Get-Config {
    param([string]$WorkspaceRoot)
    return (Read-State $WorkspaceRoot).config
}

function Set-ConfigValue {
    param([string]$WorkspaceRoot, [string]$Key, $Value)
    $state = Read-State $WorkspaceRoot
    $state.config[$Key] = $Value
    Write-State $WorkspaceRoot $state
}






