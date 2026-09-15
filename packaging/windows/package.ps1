param(
    [Parameter(Mandatory = $true)]
    [string] $Target,

    [Parameter(Mandatory = $true)]
    [ValidateSet('x86', 'x86_64', 'arm64')]
    [string] $Architecture,

    [Parameter(Mandatory = $true)]
    [string] $Version,

    [Parameter(Mandatory = $true)]
    [string] $WixExe,

    [Parameter(Mandatory = $true)]
    [string] $MakeNsisExe,

    [string] $DistRoot = 'dist'
)

$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')).Path
Set-Location -LiteralPath $projectRoot

$expectedTarget = switch ($Architecture) {
    'x86' { 'i686-pc-windows-msvc' }
    'x86_64' { 'x86_64-pc-windows-msvc' }
    'arm64' { 'aarch64-pc-windows-msvc' }
}
if ($Target -ne $expectedTarget) {
    throw "Architecture $Architecture does not match Rust target $Target"
}

$metadata = cargo metadata --locked --no-deps --format-version 1 | ConvertFrom-Json
if ($metadata.packages[0].version -ne $Version) {
    throw "Cargo version $($metadata.packages[0].version) does not match package version $Version"
}

$binary = (Resolve-Path -LiteralPath "target\$Target\release\miaozip.exe").Path
$icon = Get-ChildItem -LiteralPath "target\$Target\release\build" -Recurse -Filter 'miaozip.ico' |
    Where-Object { $_.DirectoryName -like '*\out' } |
    Select-Object -First 1
if ($null -eq $icon) {
    throw 'Cargo build output did not contain miaozip.ico'
}

$wixArchitecture = switch ($Architecture) {
    'x86' { 'x86' }
    'x86_64' { 'x64' }
    'arm64' { 'arm64' }
}
$bundleName = "miaozip-$Version-windows-$Architecture"
$distDirectory = Join-Path $projectRoot $DistRoot
$workspace = Join-Path $env:RUNNER_TEMP "miaozip-package-$Target"
$bundleDirectory = Join-Path $workspace $bundleName
if (Test-Path -LiteralPath $workspace) {
    Remove-Item -LiteralPath $workspace -Recurse -Force
}
New-Item -ItemType Directory -Path $bundleDirectory, $distDirectory -Force | Out-Null

Copy-Item -LiteralPath $binary -Destination (Join-Path $bundleDirectory 'miaozip.exe')
Copy-Item -LiteralPath 'README.md', 'LICENSE', 'THIRD_PARTY_NOTICES.md' `
    -Destination $bundleDirectory

$zipOutput = Join-Path $distDirectory "$bundleName.zip"
$tarOutput = Join-Path $distDirectory "$bundleName.tar.gz"
$exeOutput = Join-Path $distDirectory "$bundleName.exe"
$msiOutput = Join-Path $distDirectory "$bundleName.msi"
Remove-Item -LiteralPath $zipOutput, $tarOutput, $exeOutput, $msiOutput `
    -Force -ErrorAction SilentlyContinue

Compress-Archive -LiteralPath $bundleDirectory -DestinationPath $zipOutput `
    -CompressionLevel Optimal
$tar = (Get-Command tar.exe -ErrorAction Stop).Source
& $tar '-czf' $tarOutput '-C' $workspace $bundleName
if ($LASTEXITCODE -ne 0) {
    throw "tar.exe failed with exit code $LASTEXITCODE"
}

& $WixExe build 'packaging\windows\miaozip.wxs' `
    -arch $wixArchitecture `
    -d "Version=$Version" `
    -d "BinaryPath=$binary" `
    -d "IconPath=$($icon.FullName)" `
    -d "ReadmePath=$projectRoot\README.md" `
    -d "LicensePath=$projectRoot\LICENSE" `
    -d "NoticesPath=$projectRoot\THIRD_PARTY_NOTICES.md" `
    -pdbtype none `
    -out $msiOutput
if ($LASTEXITCODE -ne 0) {
    throw "WiX failed with exit code $LASTEXITCODE"
}
& $WixExe msi validate $msiOutput
if ($LASTEXITCODE -ne 0) {
    throw "MSI validation failed with exit code $LASTEXITCODE"
}

$nsisArguments = @(
    '/V2',
    "/DVERSION=$Version",
    "/DARCHITECTURE=$Architecture",
    "/DSOURCE_DIR=$bundleDirectory",
    "/DOUTPUT_FILE=$exeOutput",
    "/DICON_FILE=$($icon.FullName)",
    'packaging\windows\miaozip.nsi'
)
& $MakeNsisExe @nsisArguments
if ($LASTEXITCODE -ne 0) {
    throw "NSIS failed with exit code $LASTEXITCODE"
}

Get-Item -LiteralPath $zipOutput, $tarOutput, $exeOutput, $msiOutput |
    Select-Object Name, Length
