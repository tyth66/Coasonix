<#
.SYNOPSIS
Coagent companion script — dispatches slash commands to MCP tools.

.DESCRIPTION
Entry point for /coagent:review, /coagent:status, /coagent:cancel, /coagent:result.
Parses arguments, manages the job lifecycle, and renders output.
#>

param(
    [Parameter(Position=0, Mandatory=$true)]
    [ValidateSet('review','status','cancel','result','setup')]
    [string]$Command,

    [Parameter(ValueFromRemainingArguments=$true)]
    [string[]]$Arguments
)

$ErrorActionPreference = 'Stop'
$ROOT_DIR = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)

# ---- dot-source library ----
. (Join-Path $PSScriptRoot 'lib/state.ps1')

# ---- helpers ----
function Get-RepoRoot {
    try { return (git rev-parse --show-toplevel 2>$null) } catch { return (Get-Location).Path }
}

function Write-Output {
    param([string]$Text)
    [Console]::Out.Write($Text)
}

# ---- command: review ----
function Invoke-Review {
    $repoRoot = Get-RepoRoot
    $base = $null; $background = $false
    for ($i = 0; $i -lt $Arguments.Count; $i++) {
        if ($Arguments[$i] -eq '--base' -and $i + 1 -lt $Arguments.Count) { $base = $Arguments[++$i] }
        if ($Arguments[$i] -eq '--background') { $background = $true }
    }

    # Generate diff
    $diffsDir = Join-Path $repoRoot '.agent' 'diffs'
    New-Item -ItemType Directory -Force $diffsDir | Out-Null
    $diffPath = Join-Path $diffsDir 'current.diff'

    if ($base) {
        $null = git diff $base...HEAD 2>&1 | Out-File -FilePath $diffPath -Encoding utf8
        $targetLabel = "branch vs $base"
    } else {
        $null = git diff HEAD 2>&1 | Out-File -FilePath $diffPath -Encoding utf8
        $targetLabel = 'working-tree'
    }

    $diffSize = (Get-Item $diffPath).Length
    if ($diffSize -eq 0) {
        $untracked = git status --short --untracked-files=all 2>&1
        if (-not $untracked) { Write-Output 'Nothing to review.'; return }
    }

    # Prepare MCP call parameters as JSON
    $mcpInput = @{
        schema_version = 'review_diff_input_v1'
        goal = "Review $targetLabel changes"
        repo = @{ root = $repoRoot; base_branch = $base; working_branch = (git branch --show-current 2>&1).Trim() }
        artifacts = @{ diff_path = '.agent/diffs/current.diff' }
        permission_level = 'L1_DIFF_REVIEW'
        output_schema = 'review_result_v1'
    } | ConvertTo-Json -Depth 5 -Compress

    Write-Output $mcpInput
}

# ---- command: status ----
function Invoke-Status {
    $repoRoot = Get-RepoRoot
    $all = $Arguments -contains '--all'
    $jobs = Read-Jobs $repoRoot
    $running = @($jobs | Where-Object { $_.status -eq 'queued' -or $_.status -eq 'running' })
    $finished = @($jobs | Where-Object { $_.status -ne 'queued' -and $_.status -ne 'running' })
    $payload = @{ running = $running; latestFinished = if ($finished.Count -gt 0) { $finished[0] } else { @{} }; recent = if ($all) { $finished } else { @($finished | Select-Object -First 8) } } | ConvertTo-Json -Depth 10 -Compress
    Write-Output $payload
}

# ---- command: cancel ----
function Invoke-Cancel {
    $repoRoot = Get-RepoRoot
    $jobId = if ($Arguments.Count -gt 0) { $Arguments[0] } else { $null }

    if (-not $jobId) {
        $jobs = Read-Jobs $repoRoot
        $active = $jobs | Where-Object { $_.status -eq "queued" -or $_.status -eq "running" }
        if ($active.Count -eq 0) {
            Write-Output (@{ status = "error"; message = "No active jobs to cancel" } | ConvertTo-Json -Compress)
            return
        }
        $jobId = $active[0].id
    }

    $payload = @{ task_id = $jobId } | ConvertTo-Json -Compress
    Write-Output $payload
    Upsert-Job $repoRoot @{ id = $jobId; status = "cancelling"; phase = "cancelling" }
    Add-JobLogLine $repoRoot $jobId "Cancellation requested"
}

# ---- command: result ----
function Invoke-Result {
    $repoRoot = Get-RepoRoot
    $jobId = if ($Arguments.Count -gt 0) { $Arguments[0] } else { $null }

    if (-not $jobId) {
        $jobs = Read-Jobs $repoRoot
        $finished = $jobs | Where-Object { $_.status -eq "completed" -or $_.status -eq "failed" -or $_.status -eq "cancelled" }
        if ($finished.Count -eq 0) {
            Write-Output (@{ status = "error"; message = "No finished jobs" } | ConvertTo-Json -Compress)
            return
        }
        $jobId = $finished[0].id
    }

    $payload = @{ task_id = $jobId } | ConvertTo-Json -Compress
    Write-Output $payload
}

# ---- command: setup ----
function Invoke-Setup {
    $repoRoot = Get-RepoRoot
    $config = Get-Config $repoRoot
    $payload = @{ ready = $true; repoRoot = $repoRoot; reviewGateEnabled = $config.stopReviewGate; mcpRegistered = $true } | ConvertTo-Json -Compress
    Write-Output $payload
}

# ---- dispatch ----
switch ($Command) {
    'review'  { Invoke-Review }
    'status'  { Invoke-Status }
    'cancel'  { Invoke-Cancel }
    'result'  { Invoke-Result }
    'setup'   { Invoke-Setup }
}

