[CmdletBinding()]
Param(
    [Parameter()][string]$Architecture = "x86_64"
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true

$target = "$Architecture-pc-windows-msvc"
$binaryPath = ".\target\$target\release\soukou.exe"
$bundleRoot = ".\target\$target\release\草稿-windows-$Architecture"
$archivePath = ".\target\$target\release\soukou-windows-$Architecture.zip"

Write-Output "Building 草稿 for $target"
cargo build --release --package soukou --target $target

if (-not (Test-Path $binaryPath)) {
    throw "Expected binary was not created: $binaryPath"
}

if (Test-Path $bundleRoot) {
    Remove-Item -Path $bundleRoot -Recurse -Force
}

New-Item -Path $bundleRoot -ItemType Directory -Force | Out-Null
Copy-Item -Path $binaryPath -Destination (Join-Path $bundleRoot "soukou.exe") -Force
Copy-Item -Path ".\crates\soukou\resources\windows\Register-SoukouProtocol.ps1" -Destination (Join-Path $bundleRoot "Register-SoukouProtocol.ps1") -Force
Copy-Item -Path ".\Readme.md" -Destination (Join-Path $bundleRoot "README.md") -Force

if (Test-Path $archivePath) {
    Remove-Item -Path $archivePath -Force
}

Compress-Archive -Path "$bundleRoot\*" -DestinationPath $archivePath -Force

Write-Output "Created Windows archive: $archivePath"
