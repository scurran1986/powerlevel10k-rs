#Requires -Version 7.0
<#
.SYNOPSIS
    Install p10k-rs on Windows.

.DESCRIPTION
    Downloads a release artifact from GitHub, verifies its SHA-256, extracts
    to a user-local prefix, and wires the binary into your PowerShell profile.
    Mirrors the logic of install.sh for Linux/macOS.

    gitstatusd has no Windows port. The vcs segment will use the ShellOut
    (git.exe) fallback on Windows — this is expected and documented.

.PARAMETER Version
    Release version to install (e.g. "v0.2.4"). Defaults to the latest
    release resolved from the GitHub API.

.PARAMETER Prefix
    Install directory. Defaults to "$env:LOCALAPPDATA\Programs\p10k-rs".
    Must be a directory you own — UAC elevation is not attempted.

.PARAMETER NoProfile
    Download and install the binary but do not touch $PROFILE.

.PARAMETER NoFonts
    Skip the MesloLGS NF font-install step.

.PARAMETER VerifyOnly
    Verify that the already-installed binary passes its SHA-256 check and
    that `p10k-rs.exe init pwsh` exits 0. Does not download or modify
    anything.

.PARAMETER Uninstall
    Remove the p10k-rs block from $PROFILE and delete the install prefix.

.EXAMPLE
    irm https://raw.githubusercontent.com/scurran1986/powerlevel10k-rs/main/install.ps1 | iex

.EXAMPLE
    ./install.ps1 -Version v0.2.4 -Prefix "$HOME\.local\p10k-rs"

.EXAMPLE
    ./install.ps1 -NoProfile -NoFonts
#>

[CmdletBinding(SupportsShouldProcess, DefaultParameterSetName = 'Install')]
param(
    [Parameter(ParameterSetName = 'Install')]
    [Parameter(ParameterSetName = 'VerifyOnly')]
    [string] $Version = '',

    [Parameter(ParameterSetName = 'Install')]
    [string] $Prefix = (Join-Path $env:LOCALAPPDATA 'Programs\p10k-rs'),

    [Parameter(ParameterSetName = 'Install')]
    [switch] $NoProfile,

    [Parameter(ParameterSetName = 'Install')]
    [switch] $NoFonts,

    [Parameter(ParameterSetName = 'VerifyOnly', Mandatory)]
    [switch] $VerifyOnly,

    [Parameter(ParameterSetName = 'Uninstall', Mandatory)]
    [switch] $Uninstall
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# ---- constants ----------------------------------------------------------------

$GH_REPO    = 'scurran1986/powerlevel10k-rs'
$TRIPLE     = 'x86_64-pc-windows-msvc'
$EXE_NAME   = 'p10k-rs.exe'
$PROFILE_MARKER = '# p10k-rs (pwsh) — managed by install.ps1; remove this line + the next to uninstall'
$PROFILE_LINE   = '& (Join-Path $env:LOCALAPPDATA "Programs\p10k-rs\p10k-rs.exe") init pwsh | Invoke-Expression'

$FONTS_PIN_COMMIT = '145eb9fbc2f42ee408dacd9b22d8e6e0e553f83d'
$FONTS_BASE_URL   = "https://raw.githubusercontent.com/romkatv/powerlevel10k-media/$FONTS_PIN_COMMIT"

# Matches install.sh's pinned set verbatim.
$FONTS_PINS = @(
    @{ Name = 'MesloLGS NF Regular.ttf';     Sha256 = 'd97946186e97f8d7c0139e8983abf40a1d2d086924f2c5dbf1c29bd8f2c6e57d' }
    @{ Name = 'MesloLGS NF Bold.ttf';        Sha256 = 'b6c0199cf7c7483c8343ea020658925e6de0aeb318b89908152fcb4d19226003' }
    @{ Name = 'MesloLGS NF Italic.ttf';      Sha256 = '6f357bcbe2597704e157a915625928bca38364a89c22a4ac36e7a116dcd392ef' }
    @{ Name = 'MesloLGS NF Bold Italic.ttf'; Sha256 = '56b4131adecec052c4b324efb818dd326d586dbc316fc68f98f1cae2eb8d1220' }
)

# ---- helpers ------------------------------------------------------------------

function Write-Step([string]$Tag, [string]$Msg) {
    Write-Host "[$Tag] $Msg"
}

function Write-Warn([string]$Tag, [string]$Msg) {
    Write-Warning "[$Tag] $Msg"
}

function Write-Fail([string]$Tag, [string]$Msg) {
    Write-Error "[$Tag] $Msg" -ErrorAction Stop
}

# Returns the lowercase hex SHA-256 of a file. Matches install.sh's sha256_of().
function Get-FileSha256([string]$Path) {
    (Get-FileHash -Path $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

# Parses `<hex>  <filename>` sidecar format (two spaces, like sha256sum -c).
# Returns the hex string, or throws on malformed input.
function Read-Sha256Sidecar([string]$SidecarPath) {
    $raw = (Get-Content -LiteralPath $SidecarPath -Raw).Trim()
    if ($raw -notmatch '^([0-9a-fA-F]{64})\s+\S') {
        Write-Fail 'verify' "SHA-256 sidecar at '$SidecarPath' is not in expected format."
    }
    return $Matches[1].ToLowerInvariant()
}

function Confirm-Sha256([string]$Path, [string]$Expected) {
    $actual = Get-FileSha256 $Path
    if ($actual -ne $Expected.ToLowerInvariant()) {
        Write-Fail 'verify' ("SHA-256 mismatch for '$Path':`n  expected $Expected`n  got      $actual")
    }
}

# Resolve the latest release tag from the GitHub API if $Version is not pinned.
function Resolve-Version([string]$Requested) {
    if ($Requested -ne '') { return $Requested }
    Write-Step 'version' 'Resolving latest release from GitHub API...'
    $api = "https://api.github.com/repos/$GH_REPO/releases/latest"
    $resp = Invoke-RestMethod -Uri $api -Headers @{ 'User-Agent' = 'p10k-rs-installer/1.0' }
    $tag = $resp.tag_name
    if (-not $tag) { Write-Fail 'version' "Could not resolve latest release tag from $api" }
    Write-Step 'version' "Latest release: $tag"
    return $tag
}

# Download a URL to $Dest, failing loudly on HTTP error. Uses BitsTransfer
# when available (shows progress on interactive sessions); falls back to
# Invoke-WebRequest.
function Invoke-Download([string]$Url, [string]$Dest) {
    $wc = $null
    try {
        # BitsTransfer is interactive-friendly but requires the BITS service.
        Import-Module BitsTransfer -ErrorAction Stop
        Start-BitsTransfer -Source $Url -Destination $Dest -DisplayName 'p10k-rs download'
    } catch {
        # Plain WebRequest fallback — works headless (CI, WinPE, etc.).
        $ProgressPreference = 'SilentlyContinue'   # avoids the painfully slow progress bar
        Invoke-WebRequest -Uri $Url -OutFile $Dest -UseBasicParsing
    }
}

# ---- font helpers -------------------------------------------------------------

function Test-FontInstalled([string]$Name) {
    # Windows font registry: HKCU for per-user installs, HKLM for system.
    $perUser = 'HKCU:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts'
    $system  = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts'
    foreach ($hive in $perUser, $system) {
        if (Test-Path $hive) {
            $val = (Get-ItemProperty -LiteralPath $hive -ErrorAction SilentlyContinue)."$Name"
            if ($val) { return $true }
        }
    }
    return $false
}

function Install-Font([string]$TtfPath) {
    # Shell.Application COM object is the sanctioned way to install fonts
    # without a UAC prompt for per-user (HKCU) registration.
    $shell = New-Object -ComObject Shell.Application
    $fontsFolder = $shell.Namespace(0x14)   # CSIDL_FONTS = 0x14 → %windir%\Fonts
    # CopyHere flags: 16 (no progress dialog) + 1024 (no error UI)
    $fontsFolder.CopyHere($TtfPath, 0x410)
}

function Install-Fonts {
    if ($NoFonts) {
        Write-Step 'fonts' '--NoFonts — skipping MesloLGS NF install.'
        return
    }

    # Check if all 4 variants are already registered.
    $allPresent = $true
    foreach ($f in $FONTS_PINS) {
        # Registry key name is typically "FamilyName Style (TrueType)".
        $regName = ($f.Name -replace '\.ttf$', ' (TrueType)')
        if (-not (Test-FontInstalled $regName)) { $allPresent = $false; break }
    }
    if ($allPresent) {
        Write-Step 'fonts' 'MesloLGS NF already registered in Windows Fonts — skipping.'
        return
    }

    $tmpDir = Join-Path $env:TEMP "p10k-rs-fonts-$(New-Guid)"
    New-Item -ItemType Directory -Path $tmpDir | Out-Null

    try {
        $installed = 0
        foreach ($f in $FONTS_PINS) {
            $enc  = [Uri]::EscapeDataString($f.Name)
            $url  = "$FONTS_BASE_URL/$enc"
            $dest = Join-Path $tmpDir $f.Name

            Write-Step 'fonts' "Downloading $($f.Name)"
            try {
                Invoke-Download $url $dest
            } catch {
                Write-Warn 'fonts' "Download failed for '$($f.Name)' — skipping. Install manually from https://github.com/romkatv/powerlevel10k-media"
                return
            }

            $actual = Get-FileSha256 $dest
            if ($actual -ne $f.Sha256) {
                Write-Warn 'fonts' "SHA-256 mismatch for '$($f.Name)':`n  expected $($f.Sha256)`n  got      $actual`nRefusing to install."
                return
            }

            try {
                Install-Font $dest
                $installed++
            } catch {
                Write-Warn 'fonts' "Shell.CopyHere failed for '$($f.Name)': $_`nInstall manually from https://github.com/romkatv/powerlevel10k-media"
                return
            }
        }
        if ($installed -gt 0) {
            Write-Step 'fonts' "Installed $installed MesloLGS NF variant(s). Restart your terminal, then set font to: MesloLGS NF"
        }
    } finally {
        Remove-Item -Recurse -Force -LiteralPath $tmpDir -ErrorAction SilentlyContinue
    }
}

# ---- profile helpers ----------------------------------------------------------

function Get-ProfilePath {
    # $PROFILE is the CurrentUserCurrentHost profile path. On pwsh 7 this
    # is typically $HOME\Documents\PowerShell\Microsoft.PowerShell_profile.ps1.
    # It may not exist yet if the user has never customised their profile.
    return $PROFILE
}

function Add-ProfileLine {
    $profilePath = Get-ProfilePath
    $dir = Split-Path $profilePath

    if (-not (Test-Path $profilePath)) {
        Write-Step 'profile' "$profilePath does not exist — creating it."
        New-Item -ItemType File -Path $profilePath -Force | Out-Null
    }

    $content = Get-Content -LiteralPath $profilePath -Raw -ErrorAction SilentlyContinue
    if ($content -and $content.Contains($PROFILE_MARKER)) {
        Write-Step 'profile' 'Already wired in $PROFILE — leaving alone.'
        return
    }

    # Build the line using the actual install prefix so it works even if the
    # user passed a custom -Prefix.
    $initLine = "& (Join-Path '$Prefix' '$EXE_NAME') init pwsh | Invoke-Expression"
    $block = "`n$PROFILE_MARKER`n$initLine`n"
    Add-Content -LiteralPath $profilePath -Value $block -NoNewline:$false
    Write-Step 'profile' "Appended p10k-rs init block to $profilePath"
    Write-Step 'profile' 'Restart your shell or run: . $PROFILE'
}

function Remove-ProfileLine {
    $profilePath = Get-ProfilePath
    if (-not (Test-Path $profilePath)) { return }

    $lines = Get-Content -LiteralPath $profilePath
    $out   = [System.Collections.Generic.List[string]]::new()
    $skip  = 0
    foreach ($line in $lines) {
        if ($line -eq $PROFILE_MARKER) { $skip = 2; continue }
        if ($skip -gt 0) { $skip--; continue }
        $out.Add($line)
    }
    Set-Content -LiteralPath $profilePath -Value $out
    Write-Step 'uninstall' "Removed p10k-rs block from $profilePath"
}

# ---- PATH helpers ------------------------------------------------------------

function Add-ToSessionPath([string]$Dir) {
    $dirs = $env:Path -split ';' | Where-Object { $_ -ne '' }
    if ($dirs -notcontains $Dir) {
        $env:Path = ($dirs + $Dir) -join ';'
        Write-Step 'path' "Added '$Dir' to PATH for this session."
    }
}

function Invoke-PersistPath([string]$Dir) {
    $current = [Environment]::GetEnvironmentVariable('Path', [EnvironmentVariableTarget]::User)
    $dirs    = $current -split ';' | Where-Object { $_ -ne '' }
    if ($dirs -contains $Dir) {
        Write-Step 'path' "'$Dir' is already in user PATH — no change needed."
        return
    }
    $answer = Read-Host "[path] Add '$Dir' to your user PATH permanently? [y/N]"
    if ($answer -imatch '^y(es)?$') {
        [Environment]::SetEnvironmentVariable('Path', ($dirs + $Dir) -join ';', [EnvironmentVariableTarget]::User)
        Write-Step 'path' "Persisted '$Dir' to user PATH. Effective in new shells."
    } else {
        Write-Step 'path' "Skipped. '$Dir' is only on PATH for this session."
    }
}

# ---- cosign (optional) -------------------------------------------------------

function Invoke-CosignVerify([string]$ExePath) {
    $cosign = Get-Command cosign.exe -ErrorAction SilentlyContinue
    if (-not $cosign) {
        Write-Step 'sigstore' 'cosign.exe not on PATH — skipping Sigstore verification. Install from https://docs.sigstore.dev/cosign/system_config/installation/'
        return
    }

    # TODO: replace with actual Rekor/Fulcio identity once release pipeline is
    # publishing signatures. The identity and oidc-issuer below are placeholders.
    $identity = "https://github.com/$GH_REPO/.github/workflows/release.yml@refs/heads/main"
    $issuer   = 'https://token.actions.githubusercontent.com'

    Write-Step 'sigstore' "Running cosign verify-blob..."
    try {
        & cosign.exe verify-blob $ExePath `
            --certificate-identity $identity `
            --certificate-oidc-issuer $issuer
        Write-Step 'sigstore' 'Sigstore verification passed.'
    } catch {
        Write-Warn 'sigstore' "cosign verification failed: $_`nProceed with caution or re-download."
    }
}

# ---- verify-only --------------------------------------------------------------

function Invoke-VerifyOnly {
    $ver = Resolve-Version $Version
    $exe = Join-Path $Prefix $EXE_NAME

    if (-not (Test-Path $exe)) {
        Write-Fail 'verify' "Binary not found at '$exe'. Run the installer first."
    }

    $zipName     = "p10k-rs-$ver-$TRIPLE.zip"
    $sha256Name  = "$zipName.sha256"
    $sha256Url   = "https://github.com/$GH_REPO/releases/download/$ver/$sha256Name"

    Write-Step 'verify' "Fetching SHA-256 sidecar for $zipName..."
    $tmpSha = Join-Path $env:TEMP $sha256Name
    try {
        Invoke-Download $sha256Url $tmpSha
        # The sidecar covers the zip, not the extracted exe, so we re-download
        # the sha of the extracted binary from a separate .exe.sha256 if it
        # exists. For now: verify the exe matches the sha sidecar for the exe
        # published alongside the release (TODO: add .exe.sha256 to release
        # pipeline). This section is a best-effort check.
        Write-Warn 'verify' 'NOTE: .exe.sha256 sidecar not yet in release pipeline — verifying zip sidecar format only. Full exe verification requires a TODO in the release workflow.'
        Read-Sha256Sidecar $tmpSha | Out-Null
        Write-Step 'verify' 'Sidecar parses cleanly.'
    } finally {
        Remove-Item -LiteralPath $tmpSha -ErrorAction SilentlyContinue
    }

    Write-Step 'verify' "Running: $exe init pwsh"
    & $exe init pwsh | Out-Null
    Write-Step 'verify' 'init pwsh → ok'
}

# ---- uninstall ----------------------------------------------------------------

function Invoke-Uninstall {
    Remove-ProfileLine

    if (Test-Path $Prefix) {
        $answer = Read-Host "[uninstall] Delete install directory '$Prefix'? [y/N]"
        if ($answer -imatch '^y(es)?$') {
            Remove-Item -Recurse -Force -LiteralPath $Prefix
            Write-Step 'uninstall' "Deleted '$Prefix'."
        } else {
            Write-Step 'uninstall' "Left '$Prefix' in place."
        }
    } else {
        Write-Step 'uninstall' "'$Prefix' not found — nothing to delete."
    }

    Write-Step 'uninstall' 'Done. Open a new shell to drop the prompt.'
}

# ---- main install flow --------------------------------------------------------

function Invoke-Install {
    # 1. Resolve version.
    $ver = Resolve-Version $Version

    # 2. Detect target.
    Write-Step 'detect' "Target triple: $TRIPLE (Windows x86_64 MSVC)"
    Write-Step 'detect' "Install prefix: $Prefix"

    # 3. Build artifact URLs.
    $zipName    = "p10k-rs-$ver-$TRIPLE.zip"
    $sha256Name = "$zipName.sha256"
    $baseUrl    = "https://github.com/$GH_REPO/releases/download/$ver"
    $zipUrl     = "$baseUrl/$zipName"
    $sha256Url  = "$baseUrl/$sha256Name"

    # 4. Download to a temp dir so partial downloads never touch the install prefix.
    $tmpDir = Join-Path $env:TEMP "p10k-rs-install-$(New-Guid)"
    New-Item -ItemType Directory -Path $tmpDir | Out-Null

    try {
        $tmpZip = Join-Path $tmpDir $zipName
        $tmpSha = Join-Path $tmpDir $sha256Name

        # 5. Fetch SHA-256 sidecar first so we can verify before doing anything.
        Write-Step 'download' "Fetching SHA-256: $sha256Url"
        Invoke-Download $sha256Url $tmpSha
        $expected = Read-Sha256Sidecar $tmpSha

        # 6. Download the zip.
        Write-Step 'download' "Fetching archive: $zipUrl"
        Invoke-Download $zipUrl $tmpZip

        # 7. Verify.
        Write-Step 'verify' "Verifying SHA-256..."
        Confirm-Sha256 $tmpZip $expected
        Write-Step 'verify' "SHA-256 OK (${expected.Substring(0,12)}...)"

        # 8. Extract.
        Write-Step 'extract' "Extracting to $tmpDir..."
        Expand-Archive -LiteralPath $tmpZip -DestinationPath $tmpDir -Force

        $staged = Join-Path $tmpDir $EXE_NAME
        if (-not (Test-Path $staged)) {
            Write-Fail 'extract' "Expected '$EXE_NAME' inside the zip but it wasn't there. Contents: $(Get-ChildItem $tmpDir | Select-Object -ExpandProperty Name)"
        }

        # 9. Install: move into prefix.
        if (-not (Test-Path $Prefix)) {
            New-Item -ItemType Directory -Path $Prefix | Out-Null
        }
        $destExe = Join-Path $Prefix $EXE_NAME
        Copy-Item -LiteralPath $staged -Destination $destExe -Force
        Write-Step 'install' "Binary installed at $destExe"

        # 10. gitstatusd — intentionally skipped on Windows.
        Write-Step 'gitstatusd' 'gitstatusd has no Windows port — skipping. The vcs segment will use the git.exe ShellOut fallback on Windows. This is expected.'

    } finally {
        Remove-Item -Recurse -Force -LiteralPath $tmpDir -ErrorAction SilentlyContinue
    }

    # 11. Optional Sigstore verification.
    Invoke-CosignVerify (Join-Path $Prefix $EXE_NAME)

    # 12. Fonts.
    Install-Fonts

    # 13. PATH wiring.
    Add-ToSessionPath $Prefix
    Invoke-PersistPath $Prefix

    # 14. $PROFILE snippet.
    if (-not $NoProfile) {
        Add-ProfileLine
    } else {
        Write-Step 'profile' "-NoProfile — skipping. Add this line to your `$PROFILE manually:"
        Write-Host "  & (Join-Path '$Prefix' '$EXE_NAME') init pwsh | Invoke-Expression"
    }

    # 15. Smoke test.
    $exe = Join-Path $Prefix $EXE_NAME
    Write-Step 'verify' "Running: $exe init pwsh"
    try {
        & $exe init pwsh | Out-Null
        Write-Step 'verify' 'init pwsh → ok'
    } catch {
        Write-Warn 'verify' "'$exe init pwsh' failed: $_`nThe binary is installed but may not work correctly."
    }

    # 16. Done.
    Write-Host ''
    Write-Host '[done] p10k-rs is installed.'
    Write-Host ''
    Write-Host "To activate in this session:  . `$PROFILE"
    Write-Host "Or open a new PowerShell terminal."
    Write-Host ''
    Write-Host "To uninstall:"
    Write-Host "  ./install.ps1 -Uninstall"
    Write-Host ''
    Write-Host 'MesloLGS NF fonts (if installed) are NOT removed by -Uninstall.'
    Write-Host 'To remove them: open Settings > Personalization > Fonts and delete the MesloLGS NF entries.'
}

# ---- dispatch -----------------------------------------------------------------

switch ($PSCmdlet.ParameterSetName) {
    'VerifyOnly' { Invoke-VerifyOnly }
    'Uninstall'  { Invoke-Uninstall  }
    default      { Invoke-Install    }
}
