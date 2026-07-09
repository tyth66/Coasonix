# Coagent Plugin Tests: Companion Script
# Run: pwsh -NoProfile -File tests/companion.test.ps1

$ErrorActionPreference = "Stop"
$scriptRoot = Split-Path -Parent $PSScriptRoot
$companionScript = Join-Path $scriptRoot "scripts/coagent-companion.ps1"

$testsPassed = 0
$testsFailed = 0

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

function Assert-Equal {
    param($Expected, $Actual, [string]$Name)
    if ($Expected -eq $Actual) {
        Write-Host "  PASS: $Name" -ForegroundColor Green
        $script:testsPassed++
    } else {
        Write-Host "  FAIL: $Name" -ForegroundColor Red
        $script:testsFailed++
    }
}

Write-Host "=== Setup Command ==="
$out = pwsh -NoProfile -Command ". '$companionScript' setup" 2>&1
$result = $out | ConvertFrom-Json
Assert-True $result.ready 'Setup reports ready'
Assert-True $result.mcpRegistered 'Setup reports MCP registered'

Write-Host "=== Status Command Structure ==="
$out = pwsh -NoProfile -Command ". '$companionScript' status" 2>&1
$result = $out | ConvertFrom-Json
Assert-True ($null -ne $result.running) 'Status has running field'

Write-Host "=== Cancel Command ==="
$out = pwsh -NoProfile -Command ". '$companionScript' cancel test-job" 2>&1
$result = $out | ConvertFrom-Json
Assert-Equal 'not_implemented' $result.status 'Cancel reports not_implemented'

Write-Host "=== Result Command ==="
$out = pwsh -NoProfile -Command ". '$companionScript' result test-job" 2>&1
$result = $out | ConvertFrom-Json
Assert-Equal 'not_implemented' $result.status 'Result reports not_implemented'

Write-Host "=== Script file exists ==="
Assert-True (Test-Path $companionScript) 'Companion script file exists'

Write-Host "=== Summary ===" -ForegroundColor Cyan
Write-Host "Passed: $testsPassed, Failed: $testsFailed"
if ($testsFailed -gt 0) { exit 1 } else { exit 0 }
