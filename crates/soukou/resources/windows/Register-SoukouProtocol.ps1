[CmdletBinding()]
Param(
    [Parameter()][string]$SoukouExePath = (Join-Path $PSScriptRoot "soukou.exe")
)

$ErrorActionPreference = "Stop"

$resolvedSoukouExePath = (Resolve-Path -LiteralPath $SoukouExePath).Path
$protocolKeyPath = "HKCU:\Software\Classes\soukou"
$commandKeyPath = Join-Path $protocolKeyPath "shell\open\command"

New-Item -Path $protocolKeyPath -Force | Out-Null
New-Item -Path $commandKeyPath -Force | Out-Null
Set-Item -Path $protocolKeyPath -Value "URL:Soukou Auth Callback"
New-ItemProperty -Path $protocolKeyPath -Name "URL Protocol" -Value "" -PropertyType String -Force | Out-Null
Set-Item -Path $commandKeyPath -Value "`"$resolvedSoukouExePath`" `"%1`""

Write-Output "Registered soukou:// protocol handler for $resolvedSoukouExePath"
