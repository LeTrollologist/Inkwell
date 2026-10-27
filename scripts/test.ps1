# Runs the Rust test suite.
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\_env.ps1"
$repoRoot = Split-Path $PSScriptRoot -Parent
Push-Location $repoRoot
try {
    & cargo test @args
    exit $LASTEXITCODE
} finally {
    Pop-Location
}
