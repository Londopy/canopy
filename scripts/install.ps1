<#
.SYNOPSIS
    Installs Canopy from its latest GitHub release.

.DESCRIPTION
    One-liner (PowerShell 5.1 or 7):
        irm https://github.com/Londopy/canopy/releases/latest/download/install.ps1 | iex

    With options, run it as a script block:
        & ([scriptblock]::Create((irm https://github.com/Londopy/canopy/releases/latest/download/install.ps1))) -Msi

    What it does:
      - picks the right download for this PC (x64, x86 or arm64)
      - verifies it against the release's SHA256SUMS.txt and refuses on a mismatch
      - runs the setup.exe silently (per-user, no admin prompt)
      - -Msi or -AllUsers: runs the .msi instead (asks for admin)
      - -Portable: no installer, just the exe in a folder plus a Start menu shortcut
      - -Lite: Canopy Lite (web mode only, smaller), as a portable exe

.PARAMETER Version
    Install this version (for example 1.2.3) instead of the latest.
.PARAMETER Msi
    Use the .msi instead of the setup.exe.
.PARAMETER AllUsers
    Install for every user on this PC, with the .msi (asks for admin).
.PARAMETER Lite
    Install Canopy Lite (web mode only) as a portable exe.
.PARAMETER Portable
    Skip the installer: put the exe in -Dir and add a Start menu shortcut.
.PARAMETER Dir
    Folder for -Portable. Default: %LOCALAPPDATA%\Programs\Canopy
.PARAMETER AddToPath
    With -Portable, add the folder to your user PATH.
.PARAMETER NoShortcut
    With -Portable, don't create the Start menu shortcut.
.PARAMETER DryRun
    Show what would be downloaded and run, then stop.
#>
# Write-Host is right for an interactive installer; the parameters are read
# inside Install-App, which the analyzer can't see
[Diagnostics.CodeAnalysis.SuppressMessageAttribute('PSAvoidUsingWriteHost', '')]
[Diagnostics.CodeAnalysis.SuppressMessageAttribute('PSReviewUnusedParameter', '')]
param(
    [string]$Version,
    [switch]$Msi,
    [switch]$AllUsers,
    [switch]$Portable,
    [switch]$Lite,
    [string]$Dir,
    [switch]$AddToPath,
    [switch]$NoShortcut,
    [switch]$DryRun,
    # for testing against a mock API; leave as is
    [string]$ApiBase = 'https://api.github.com'
)

# no `exit` anywhere: under `irm | iex` it would close the user's window

$AppName = 'Canopy'
$Repo = 'Londopy/canopy'

function Get-NativeArch {
    # the registry has the real CPU even when this PowerShell is emulated
    # (x64 PowerShell on an arm64 PC reports AMD64 in its own environment)
    $key = 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment'
    $arch = $null
    try { $arch = (Get-ItemProperty -Path $key -Name PROCESSOR_ARCHITECTURE -ErrorAction Stop).PROCESSOR_ARCHITECTURE } catch { $arch = $null }
    if (-not $arch) { $arch = $env:PROCESSOR_ARCHITEW6432 }
    if (-not $arch) { $arch = $env:PROCESSOR_ARCHITECTURE }
    switch ($arch) {
        'ARM64' { return 'arm64' }
        'x86' { return 'x86' }
        default { return 'x64' }
    }
}

function Get-Release {
    param([string]$Tag)
    $headers = @{ 'User-Agent' = "$AppName-installer"; 'Accept' = 'application/vnd.github+json' }
    if ($Tag) {
        $url = "$ApiBase/repos/$Repo/releases/tags/$Tag"
    } else {
        $url = "$ApiBase/repos/$Repo/releases/latest"
    }
    Invoke-RestMethod -Uri $url -Headers $headers -UseBasicParsing
}

function Get-ChecksumTable {
    param($Release)
    $asset = $Release.assets | Where-Object { $_.name -eq 'SHA256SUMS.txt' } | Select-Object -First 1
    if (-not $asset) { throw "This release has no SHA256SUMS.txt, so downloads can't be verified." }
    $text = (Invoke-WebRequest -Uri $asset.browser_download_url -UseBasicParsing).Content
    if ($text -is [byte[]]) { $text = [System.Text.Encoding]::UTF8.GetString($text) }
    $sums = @{}
    foreach ($line in ($text -split "`r?`n")) {
        $parts = $line.Trim() -split '\s+', 2
        if ($parts.Count -eq 2) { $sums[$parts[1].TrimStart('*')] = $parts[0].ToLowerInvariant() }
    }
    return $sums
}

# which file to download; $null when the release doesn't have it. the names
# are tauri's: Canopy_<ver>_<arch>-setup.exe (NSIS, per-user) and
# Canopy_<ver>_<arch>_en-US.msi, plus the release's Canopy-v<ver>-<arch>-portable.exe
# and Canopy-Lite-v<ver>-<arch>-portable.exe
function Select-Download {
    param($Names, [string]$Arch, [bool]$WantMsi, [bool]$WantPortable, [bool]$WantLite = $false)
    if ($WantLite) {
        $exe = $Names | Where-Object { $_ -like "Canopy-Lite-v*-$Arch-portable.exe" } | Select-Object -First 1
        if ($exe) { return @{ Kind = 'exe'; Name = $exe } }
        return $null
    }
    if ($WantPortable) {
        $exe = $Names | Where-Object { $_ -like "Canopy-v*-$Arch-portable.exe" } | Select-Object -First 1
        if ($exe) { return @{ Kind = 'exe'; Name = $exe } }
        return $null
    }
    if (-not $WantMsi) {
        $setup = $Names | Where-Object { $_ -like "*_$Arch-setup.exe" } | Select-Object -First 1
        if ($setup) { return @{ Kind = 'setup'; Name = $setup } }
    }
    $msiName = $Names | Where-Object { $_ -like "*_${Arch}_en-US.msi" } | Select-Object -First 1
    if ($msiName) { return @{ Kind = 'msi'; Name = $msiName } }
    # a prerelease ships no MSIs: fall back to the setup
    $setup = $Names | Where-Object { $_ -like "*_$Arch-setup.exe" } | Select-Object -First 1
    if ($setup) { return @{ Kind = 'setup'; Name = $setup } }
    return $null
}

function Save-Verified {
    param($Release, [string]$Name, $Sums)
    if (-not $Sums.ContainsKey($Name)) { throw "SHA256SUMS.txt has no entry for $Name; not installing it." }
    $asset = $Release.assets | Where-Object { $_.name -eq $Name } | Select-Object -First 1
    $path = Join-Path ([System.IO.Path]::GetTempPath()) $Name
    Write-Host "Downloading $Name ..."
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $path -UseBasicParsing
    $actual = (Get-FileHash -Path $path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $Sums[$Name]) {
        Remove-Item -Path $path -Force -ErrorAction SilentlyContinue
        throw "SHA-256 mismatch for $Name (expected $($Sums[$Name]), got $actual). Nothing was installed."
    }
    Write-Host 'SHA-256 verified.'
    return $path
}

function Add-UserPath {
    param([string]$Folder)
    $current = [Environment]::GetEnvironmentVariable('Path', 'User')
    $parts = @()
    if ($current) { $parts = $current -split ';' | Where-Object { $_ } }
    if ($parts -contains $Folder) { return }
    [Environment]::SetEnvironmentVariable('Path', (($parts + $Folder) -join ';'), 'User')
    Write-Host "Added $Folder to your PATH (open a new terminal to use it)."
}

function Add-StartMenuShortcut {
    param([string]$Target, [string]$Name = $AppName)
    $programs = [Environment]::GetFolderPath('Programs')
    $lnk = Join-Path $programs "$Name.lnk"
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($lnk)
    $shortcut.TargetPath = $Target
    $shortcut.WorkingDirectory = Split-Path -Parent $Target
    $shortcut.Save()
    Write-Host "Start menu shortcut: $lnk"
}

function Install-App {
    $ErrorActionPreference = 'Stop'
    # PowerShell 5.1 shows a very slow progress bar while downloading
    $ProgressPreference = 'SilentlyContinue'
    # PowerShell 5.1 on older .NET doesn't offer TLS 1.2 by default; GitHub needs it
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

    $arch = Get-NativeArch
    $tag = $null
    if ($Version) { $tag = 'v' + $Version.TrimStart('v') }
    $release = Get-Release -Tag $tag
    $names = @($release.assets | ForEach-Object { $_.name })
    $pick = Select-Download -Names $names -Arch $arch -WantMsi ($Msi.IsPresent -or $AllUsers.IsPresent) -WantPortable $Portable.IsPresent -WantLite $Lite.IsPresent
    if (-not $pick) {
        throw "$($release.tag_name) has no download for this PC ($arch). See https://github.com/$Repo/releases"
    }
    Write-Host "$AppName $($release.tag_name) for $arch -> $($pick.Name)"
    if ($DryRun) { return $pick }

    $sums = Get-ChecksumTable -Release $release
    $file = Save-Verified -Release $release -Name $pick.Name -Sums $sums

    switch ($pick.Kind) {
        'setup' {
            # tauri's NSIS installer: /S is silent; it always installs for the current user
            $p = Start-Process -FilePath $file -Wait -PassThru -ArgumentList @('/S')
            if ($p.ExitCode -ne 0) { throw "Setup exited with code $($p.ExitCode)." }
        }
        'msi' {
            # tauri's MSI installs for every user, so Windows asks for admin
            $msiArgs = "/i `"$file`" /passive /norestart"
            $p = Start-Process -FilePath 'msiexec.exe' -Wait -PassThru -ArgumentList $msiArgs
            # 3010 = installed, restart recommended
            if ($p.ExitCode -ne 0 -and $p.ExitCode -ne 3010) { throw "msiexec exited with code $($p.ExitCode)." }
        }
        'exe' {
            $target = $Dir
            $folderName = $AppName
            if ($Lite) { $folderName = "$AppName Lite" }
            if (-not $target) { $target = Join-Path $env:LOCALAPPDATA "Programs\$folderName" }
            New-Item -ItemType Directory -Force -Path $target | Out-Null
            $dest = Join-Path $target "$AppName.exe"
            Copy-Item -Path $file -Destination $dest -Force
            Write-Host "Installed to $dest"
            Write-Host "Settings live in %APPDATA%\$AppName. Put an empty file named 'portable' next to the exe to keep them in that folder instead."
            # Lite gets its own shortcut, so it never replaces the full app's
            if (-not $NoShortcut) { Add-StartMenuShortcut -Target $dest -Name $folderName }
            if ($AddToPath) { Add-UserPath -Folder $target }
        }
    }
    Remove-Item -Path $file -Force -ErrorAction SilentlyContinue
    Write-Host "$AppName $($release.tag_name) is installed."
}

Install-App
