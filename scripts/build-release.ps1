[CmdletBinding()]
param(
    [string]$Target = 'x86_64-pc-windows-msvc',
    [switch]$Interactive
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$targetDirectory = Join-Path $projectRoot 'target'
$outputDirectory = Join-Path $projectRoot 'dist'

function Invoke-Cargo {
    param([string[]]$CargoArguments)
    & cargo @CargoArguments
    if ($LASTEXITCODE -ne 0) {
        throw "cargo $($CargoArguments -join ' ') failed with exit code $LASTEXITCODE"
    }
}

Push-Location $projectRoot
try {
    Invoke-Cargo -CargoArguments @('fmt', '--all', '--', '--check')
    Invoke-Cargo -CargoArguments @('clippy', '--all-targets', '--locked', '--', '-D', 'warnings')
    Invoke-Cargo -CargoArguments @('test', '--all-targets', '--locked')
    if ($Interactive) {
        Invoke-Cargo -CargoArguments @('test', '--bin', 'HanEngCursor', '--locked', '--', '--ignored', '--test-threads=1', '--nocapture')
    }
    Invoke-Cargo -CargoArguments @('build', '--release', '--locked', '--target', $Target, '--target-dir', $targetDirectory)

    $metadataText = & cargo metadata --no-deps --format-version 1 --locked
    if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed' }
    $metadata = $metadataText | ConvertFrom-Json
    $package = $metadata.packages | Where-Object { $_.name -eq 'hanengcursor' }
    if (-not $package) { throw 'hanengcursor package not found' }
    $packageName = "HanEngCursor-v$($package.version)-$Target"
    $packageDirectory = Join-Path $outputDirectory $packageName
    $null = New-Item -ItemType Directory -Path $packageDirectory -Force
    $null = New-Item -ItemType Directory -Path (Join-Path $packageDirectory 'docs') -Force
    $executable = Join-Path $targetDirectory "$Target/release/HanEngCursor.exe"
    Copy-Item -LiteralPath $executable -Destination (Join-Path $packageDirectory 'HanEngCursor.exe') -Force
    Copy-Item -LiteralPath (Join-Path $projectRoot 'README.md') -Destination $packageDirectory -Force
    Copy-Item -LiteralPath (Join-Path $projectRoot 'CHANGELOG.md') -Destination $packageDirectory -Force
    foreach ($document in @('catchtype.md', 'error-handling.md', 'validation.md')) {
        Copy-Item -LiteralPath (Join-Path $projectRoot "docs/$document") -Destination (Join-Path $packageDirectory 'docs') -Force
    }
    $checksum = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
    "$checksum  HanEngCursor.exe" | Set-Content -LiteralPath (Join-Path $packageDirectory 'SHA256SUMS.txt') -Encoding ASCII
    $toolchain = & rustc --version
    if ($LASTEXITCODE -ne 0) { throw 'rustc --version failed' }
    @("Version: $($package.version)", "Target: $Target", "Toolchain: $toolchain", "Bytes: $((Get-Item -LiteralPath $executable).Length)") |
        Set-Content -LiteralPath (Join-Path $packageDirectory 'BUILDINFO.txt') -Encoding ASCII
    $archive = Join-Path $outputDirectory "$packageName.zip"
    Compress-Archive -Path (Join-Path $packageDirectory '*') -DestinationPath $archive -Force
    Write-Output "Release package: $archive"
    Write-Output "Executable SHA256: $checksum"
}
finally {
    Pop-Location
}
