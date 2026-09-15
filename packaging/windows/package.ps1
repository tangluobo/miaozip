param(
    [Parameter(Mandatory = $true)] [string] $Target,
    [Parameter(Mandatory = $true)] [string] $WixExe,
    [string] $DistRoot = "dist"
)

$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')).Path
Set-Location -LiteralPath $projectRoot

$metadata = cargo metadata --locked --no-deps --format-version 1 | ConvertFrom-Json
$version = $metadata.packages[0].version
$binary = (Resolve-Path -LiteralPath "target\$Target\release\miaozip.exe").Path
$icon = Get-ChildItem -LiteralPath "target\$Target\release\build" -Recurse -Filter 'miaozip.ico' |
    Where-Object { $_.DirectoryName -like '*\out' } |
    Select-Object -First 1
if ($null -eq $icon) {
    throw 'Cargo build output did not contain miaozip.ico'
}

$architecture = switch ($Target) {
    'i686-pc-windows-msvc' { 'x86' }
    'x86_64-pc-windows-msvc' { 'x64' }
    'aarch64-pc-windows-msvc' { 'arm64' }
    default { throw "Unsupported Windows target: $Target" }
}

$dist = Join-Path $projectRoot "$DistRoot\$Target"
$portable = Join-Path $env:RUNNER_TEMP "miaozip-portable-$Target"
New-Item -ItemType Directory -Path $dist, $portable -Force | Out-Null

$standalone = Join-Path $dist "miaozip-$Target.exe"
Copy-Item -LiteralPath $binary -Destination $standalone -Force
Copy-Item -LiteralPath $binary -Destination (Join-Path $portable 'miaozip.exe') -Force
Copy-Item -LiteralPath 'README.md', 'LICENSE' -Destination $portable -Force
Compress-Archive -Path (Join-Path $portable '*') -DestinationPath (Join-Path $dist "miaozip-$Target.zip") -Force

$msi = Join-Path $dist "miaozip-$Target.msi"
& $WixExe build 'packaging\windows\miaozip.wxs' `
    -arch $architecture `
    -d "Version=$version" `
    -d "BinaryPath=$binary" `
    -d "IconPath=$($icon.FullName)" `
    -d "LicensePath=$projectRoot\LICENSE" `
    -pdbtype none `
    -out $msi
if ($LASTEXITCODE -ne 0) {
    throw "WiX failed with exit code $LASTEXITCODE"
}
& $WixExe msi validate $msi
if ($LASTEXITCODE -ne 0) {
    throw "MSI validation failed with exit code $LASTEXITCODE"
}

Get-Item -LiteralPath $standalone, (Join-Path $dist "miaozip-$Target.zip"), $msi |
    Select-Object Name, Length
