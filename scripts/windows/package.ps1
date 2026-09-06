[CmdletBinding()]
param(
    [string]$Version = '0.5.2-beta',
    [switch]$SkipBuild,
    [switch]$KeepStaging
)

$ErrorActionPreference = 'Stop'

$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$targetTriple = 'x86_64-pc-windows-msvc'
$buildDirectory = Join-Path $projectRoot "target\$targetTriple\release"
$distDirectory = Join-Path $projectRoot 'dist'
$stagingDirectory = Join-Path $projectRoot 'release-staging\windows-x64'
$installerScript = Join-Path $PSScriptRoot 'LecooControlCenter.iss'
$isccCandidates = @(
    (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'),
    'C:\Program Files (x86)\Inno Setup 6\ISCC.exe',
    'C:\Program Files\Inno Setup 6\ISCC.exe'
)

function Require-File {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Required release file is missing: $Path"
    }
}

function Reset-StagingDirectory {
    $resolvedRoot = [System.IO.Path]::GetFullPath($projectRoot).TrimEnd('\')
    $resolvedStage = [System.IO.Path]::GetFullPath($stagingDirectory)
    if (-not $resolvedStage.StartsWith("$resolvedRoot\", [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to clean a staging directory outside the project: $resolvedStage"
    }

    if (Test-Path -LiteralPath $resolvedStage) {
        Remove-Item -LiteralPath $resolvedStage -Recurse -Force
    }
    New-Item -ItemType Directory -Path $resolvedStage -Force | Out-Null
}

function Find-Iscc {
    foreach ($candidate in $isccCandidates) {
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            return $candidate
        }
    }
    throw 'Inno Setup 6 was not found. Install it, then rerun this script.'
}

function Invoke-ReleaseBuild {
    param([string]$Package)

    & rustup run stable cargo build --release --target $targetTriple -p $Package
    if ($LASTEXITCODE -ne 0) {
        throw "Release build failed for package: $Package"
    }
}

Push-Location $projectRoot
try {
    if (-not $SkipBuild) {
        Invoke-ReleaseBuild 'lecoo-ec-daemon'
        Invoke-ReleaseBuild 'lecoo-ctrl'
        Invoke-ReleaseBuild 'gui'
    }

    $requiredBuildFiles = @(
        (Join-Path $buildDirectory 'lecoo-ec-daemon.exe'),
        (Join-Path $buildDirectory 'lecoo-ctrl.exe'),
        (Join-Path $buildDirectory 'lecoo-control-center.exe'),
        (Join-Path $projectRoot 'libs\inpoutx64.dll'),
        (Join-Path $projectRoot 'scripts\windows\install.bat'),
        (Join-Path $projectRoot 'scripts\windows\uninstall.bat'),
        (Join-Path $projectRoot 'scripts\windows\languages\ChineseSimplified.isl'),
        (Join-Path $projectRoot 'LICENSE'),
        (Join-Path $projectRoot 'README.md'),
        (Join-Path $projectRoot 'README_CN.md'),
        $installerScript
    )
    foreach ($file in $requiredBuildFiles) {
        Require-File $file
    }

    New-Item -ItemType Directory -Path $distDirectory -Force | Out-Null
    Reset-StagingDirectory

    Copy-Item -LiteralPath (Join-Path $buildDirectory 'lecoo-ec-daemon.exe') -Destination $stagingDirectory
    Copy-Item -LiteralPath (Join-Path $buildDirectory 'lecoo-ctrl.exe') -Destination $stagingDirectory
    Copy-Item -LiteralPath (Join-Path $buildDirectory 'lecoo-control-center.exe') -Destination $stagingDirectory
    Copy-Item -LiteralPath (Join-Path $projectRoot 'libs\inpoutx64.dll') -Destination $stagingDirectory
    Copy-Item -LiteralPath (Join-Path $projectRoot 'scripts\windows\install.bat') -Destination $stagingDirectory
    Copy-Item -LiteralPath (Join-Path $projectRoot 'scripts\windows\uninstall.bat') -Destination $stagingDirectory
    Copy-Item -LiteralPath (Join-Path $projectRoot 'LICENSE') -Destination $stagingDirectory
    Copy-Item -LiteralPath (Join-Path $projectRoot 'README.md') -Destination $stagingDirectory
    Copy-Item -LiteralPath (Join-Path $projectRoot 'README_CN.md') -Destination $stagingDirectory

    $zipPath = Join-Path $distDirectory "Lecoo-Control-Center-$Version-Windows-x64.zip"
    Compress-Archive -Path (Join-Path $stagingDirectory '*') -DestinationPath $zipPath -Force

    $iscc = Find-Iscc
    & $iscc "/DAppVersion=$Version" "/DSourceDir=$buildDirectory" "/DProjectRoot=$projectRoot" "/DOutputDir=$distDirectory" $installerScript
    if ($LASTEXITCODE -ne 0) {
        throw "Inno Setup compilation failed with exit code $LASTEXITCODE."
    }

    $setupPath = Join-Path $distDirectory "Lecoo-Control-Center-$Version-Windows-x64-Setup.exe"
    Require-File $zipPath
    Require-File $setupPath

    $artifacts = @($setupPath, $zipPath)
    $checksums = @()
    $individualChecksumPaths = @()
    foreach ($artifact in $artifacts) {
        $hash = Get-FileHash -LiteralPath $artifact -Algorithm SHA256
        $checksum = "{0} *{1}" -f $hash.Hash.ToLowerInvariant(), [System.IO.Path]::GetFileName($artifact)
        $checksums += $checksum
        $individualChecksumPath = "$artifact.sha256"
        [System.IO.File]::WriteAllText($individualChecksumPath, "$checksum`n", [System.Text.UTF8Encoding]::new($false))
        $individualChecksumPaths += $individualChecksumPath
    }
    $checksumPath = Join-Path $distDirectory "Lecoo-Control-Center-$Version-Windows-x64.sha256"
    [System.IO.File]::WriteAllLines($checksumPath, $checksums, [System.Text.UTF8Encoding]::new($false))

    @($setupPath, $zipPath, $checksumPath) + $individualChecksumPaths |
        ForEach-Object { Get-Item -LiteralPath $_ } |
        Select-Object Name, Length, LastWriteTime |
        Format-Table -AutoSize
}
finally {
    Pop-Location
    if (-not $KeepStaging -and (Test-Path -LiteralPath $stagingDirectory)) {
        Remove-Item -LiteralPath $stagingDirectory -Recurse -Force
    }
}
