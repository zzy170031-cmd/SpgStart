param(
    [string]$TargetRoot = "$HOME\.codex\skills"
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$source = Join-Path $repoRoot "skills\five-agent-governance"
$target = Join-Path $TargetRoot "five-agent-governance"

if (-not (Test-Path -LiteralPath $source)) {
    throw "Source skill folder not found: $source"
}

New-Item -ItemType Directory -Force -Path $TargetRoot | Out-Null

if (Test-Path -LiteralPath $target) {
    Remove-Item -LiteralPath $target -Recurse -Force
}

Copy-Item -LiteralPath $source -Destination $target -Recurse

Write-Output "Installed five-agent-governance to $target"
