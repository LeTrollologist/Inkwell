# Builds the release executable at target\release\inkwell.exe.
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\_env.ps1"
$repoRoot = Split-Path $PSScriptRoot -Parent
Push-Location $repoRoot
try {
    & cargo build --release @args
    exit $LASTEXITCODE
} finally {
    Pop-Location
}
