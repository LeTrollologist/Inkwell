# Runs Inkwell in development mode.
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\_env.ps1"
$repoRoot = Split-Path $PSScriptRoot -Parent
Push-Location $repoRoot
try {
    & cargo run @args
    exit $LASTEXITCODE
} finally {
    Pop-Location
}
