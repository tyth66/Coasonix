# Coagent Plugin Tests: State Layer
# Run: pwsh -NoProfile -File tests/state.test.ps1

$ErrorActionPreference = "Stop"
$scriptRoot = Split-Path -Parent $PSScriptRoot
. (Join-Path $scriptRoot "scripts/lib/state.ps1")

# Use a unique temp workspace for each test run
$testWs = Join-Path $env:TEMP ("coagent-test-" + (Get-Random))

# Override plugin data dir so tests never touch real state
function global:Get-PluginDataDir {
    return Join-Path $testWs "plugin-data"
}

# Clean up any previous test state
Get-ChildItem $env:TEMP -Directory -Filter "coagent-test-*" -ErrorAction SilentlyContinue | Remove-Item -Recurse -Force
Remove-Item -Recurse -Force $testWs -ErrorAction SilentlyContinue

$testsPassed = 0
$testsFailed = 0

function Assert-Equal {
    param($Expected, $Actual, [string]$Name)
    if ($Expected -eq $Actual) {
        Write-Host "  PASS: $Name" -ForegroundColor Green
        $script:testsPassed++
    } else {
        Write-Host "  FAIL: $Name (expected: $Expected, actual: $Actual)" -ForegroundColor Red
        $script:testsFailed++
    }
}

function Assert-True {
    param([bool]$Condition, [string]$Name)
    if ($Condition) {
        Write-Host "  PASS: $Name" -ForegroundColor Green
        $script:testsPassed++
    } else {
        Write-Host "  FAIL: $Name" -ForegroundColor Red
        $script:testsFailed++
    }
}

function Assert-NotNull {
    param($Value, [string]$Name)
    if ($null -ne $Value) {
        Write-Host "  PASS: $Name" -ForegroundColor Green
        $script:testsPassed++
    } else {
        Write-Host "  FAIL: $Name (value is null)" -ForegroundColor Red
        $script:testsFailed++
    }
}

# ---- Tests ----
Write-Host "=== JobId Generation ==="
$id1 = New-JobId -Prefix "test"
$id2 = New-JobId -Prefix "test"
Assert-True ($id1 -match '^test-') 'JobId has prefix'
Assert-True ($id1 -ne $id2) 'JobId is unique'

Write-Host "=== State Initialization ==="
$state = Read-State $testWs
Assert-Equal 1 $state.version 'Default state version'
Assert-Equal 0 $state.jobs.Count 'Default state has no jobs'
Assert-Equal $false $state.config.stopReviewGate 'Default stopReviewGate is false'

Write-Host "=== Config Read/Write ==="
Set-ConfigValue $testWs 'stopReviewGate' $true
$config = Get-Config $testWs
Assert-Equal $true $config.stopReviewGate 'stopReviewGate set to true'

Write-Host "=== Job Lifecycle ==="
$jobId = New-JobId -Prefix "review"
Upsert-Job $testWs @{ id = $jobId; kind = 'review'; status = 'queued'; phase = 'queued'; title = 'Test'; summary = 'Test' }
Upsert-Job $testWs @{ id = $jobId; status = 'running'; phase = 'starting' }
Upsert-Job $testWs @{ id = $jobId; status = 'completed'; phase = 'done' }

$jobs = Read-Jobs $testWs
$job = $jobs | Where-Object { $_.id -eq $jobId }
Assert-NotNull $job 'Job appears in list'
Assert-Equal 'completed' $job.status 'Final status is completed'
Assert-Equal 'done' $job.phase 'Final phase is done'

Write-Host "=== Job Log ==="
Add-JobLogLine $testWs $jobId 'Line 1'
Add-JobLogLine $testWs $jobId 'Line 2'
$logFile = Get-JobLogFile $testWs $jobId
$logContent = Get-Content $logFile
Assert-Equal 2 $logContent.Count 'Log has 2 lines'
Assert-True ($logContent[0] -match 'Line 1') 'First log line contains message'

Write-Host "=== Job File Persistence ==="
Write-JobFile $testWs $jobId @{ id = $jobId; result = 'done' }
$stored = Read-JobFile $testWs $jobId
Assert-Equal $jobId $stored.id 'Job file has correct id'
Assert-Equal 'done' $stored.result 'Job file has result'

# ---- Cleanup ----
Remove-Item -Recurse -Force $testWs -ErrorAction SilentlyContinue

# ---- Summary ----
Write-Host "=== Summary ===" -ForegroundColor Cyan
Write-Host "Passed: $testsPassed, Failed: $testsFailed"
if ($testsFailed -gt 0) { exit 1 } else { exit 0 }
