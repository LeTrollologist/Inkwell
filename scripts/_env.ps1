# Puts a project-local MinGW toolchain on PATH when present.
# (Windows + GNU Rust target needs gcc to link; see README.)
$repoRoot = Split-Path $PSScriptRoot -Parent
$mingwBin = Join-Path $repoRoot 'tools\w64devkit\bin'

if (Test-Path (Join-Path $mingwBin 'gcc.exe')) {
    $env:PATH = "$mingwBin;$env:PATH"
}
