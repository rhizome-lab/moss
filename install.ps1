# Moss CLI installer for Windows
# Usage: irm https://raw.githubusercontent.com/pterror/moss/master/install.ps1 | iex

$ErrorActionPreference = "Stop"

$Repo = "pterror/moss"
$InstallDir = "$env:LOCALAPPDATA\moss"

# Create install directory
if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

# Get latest version
Write-Host "Fetching latest release..."
$Release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
$Version = $Release.tag_name

Write-Host "Installing moss $Version..."

# Download
$Asset = "moss-x86_64-pc-windows-msvc.zip"
$Url = "https://github.com/$Repo/releases/download/$Version/$Asset"
$ZipPath = "$env:TEMP\moss.zip"

Invoke-WebRequest -Uri $Url -OutFile $ZipPath

# Extract
Expand-Archive -Path $ZipPath -DestinationPath $InstallDir -Force
Remove-Item $ZipPath

# Add to PATH if not already there
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    Write-Host "Adding $InstallDir to PATH..."
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
    $env:Path = "$env:Path;$InstallDir"
}

Write-Host ""
Write-Host "Installed moss $Version to $InstallDir"
Write-Host "Restart your terminal, then run 'moss --help' to get started"
