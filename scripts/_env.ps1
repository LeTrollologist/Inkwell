# Sets up the native Windows toolchain for the active Rust default.
# - MSVC: imports the Visual Studio x64 environment (SDK + link.exe).
# - GNU:  puts the project-local MinGW-w64 toolchain on PATH.
$repoRoot = Split-Path $PSScriptRoot -Parent

$rustHost = (& rustc -vV | Select-String '^host:' | Select-Object -First 1).Line
if ($rustHost -match 'msvc') {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    $install = $null
    if (Test-Path $vswhere) {
        $install = (& $vswhere -latest -products * `
            -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
            -property installationPath | Select-Object -First 1)
    }
    if (-not $install) {
        $candidates = @(
            'C:\Program Files\Microsoft Visual Studio\2022\Community',
            'C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools'
        )
        $install = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
    }
    $vcvars = if ($install) {
        Join-Path $install 'VC\Auxiliary\Build\vcvars64.bat'
    }
    if ($vcvars -and (Test-Path $vcvars)) {
        $vsInstaller = Split-Path $vswhere -Parent
        if (Test-Path (Join-Path $vsInstaller 'vswhere.exe')) {
            $env:PATH = "$vsInstaller;$env:PATH"
        }
        $environment = & cmd.exe /c "call `"$vcvars`" >nul && set"
        foreach ($line in $environment) {
            if ($line -match '^([^=]+)=(.*)$' -and $matches[1] -notmatch '^=') {
                [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process')
            }
        }
        return
    }
    Write-Warning 'MSVC toolchain selected but vcvars64.bat was not found. Install Visual Studio Build Tools with the Windows SDK.'
    return
}

$mingwBin = Join-Path $repoRoot 'tools\w64devkit\bin'
if (Test-Path (Join-Path $mingwBin 'gcc.exe')) {
    $env:PATH = "$mingwBin;$env:PATH"
}
