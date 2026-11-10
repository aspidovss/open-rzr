# Build open-rzr for Windows (Rust + Tauri).
#   powershell -ExecutionPolicy Bypass -File packaging\windows\build.ps1           # exe + portable zip
#   powershell -ExecutionPolicy Bypass -File packaging\windows\build.ps1 -Installer # + NSIS setup.exe
# Needs: rustup (MSVC toolchain) and Visual Studio Build Tools with "Desktop development with C++".
param([switch]$Installer)
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..\..")

function Step($t) { Write-Host "== $t" -ForegroundColor Cyan }
function Check() { if ($LASTEXITCODE -ne 0) { throw "failed (exit $LASTEXITCODE)" } }

# A PowerShell window opened before rustup was installed does not see ~/.cargo/bin yet.
$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if ((Test-Path $cargoBin) -and ($env:Path -notlike "*$cargoBin*")) { $env:Path = "$cargoBin;$env:Path" }
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
  Write-Host "Rust not found. Install it, open a NEW PowerShell window and run this script again:" -ForegroundColor Yellow
  Write-Host "  winget install -e --id Rustlang.Rustup"
  Write-Host "  rustup default stable-msvc"
  exit 1
}
if (-not (& rustup toolchain list 2>$null)) {
  Step "rustup: installing stable-msvc toolchain"
  rustup default stable-msvc; Check
}
cargo --version

Step "tests (protocol, hal, core)"
cargo test -p rzr-protocol -p rzr-hal -p rzr-core --release; Check

Step "build open-rzr.exe + open-rzr-cli.exe + orzr-cli.exe"
cargo build --release -p rzr-cli -p open-rzr; Check

$ver = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
Step "smoke test $ver"
& .\target\release\open-rzr-cli.exe version; Check
& .\target\release\open-rzr-cli.exe --mock info; Check
& .\target\release\orzr-cli.exe --mock -ls; Check

New-Item -ItemType Directory -Force dist | Out-Null
$stage = "dist\open-rzr-$ver"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory $stage | Out-Null
Copy-Item target\release\open-rzr.exe, target\release\open-rzr-cli.exe, target\release\orzr-cli.exe, README.md, README.ru.md, LICENSE $stage
$zip = "dist\open-rzr-$ver-windows-x64-portable.zip"
if (Test-Path $zip) { Remove-Item $zip }
Compress-Archive -Path "$stage\*" -DestinationPath $zip
Write-Host "Portable: $zip"

if ($Installer) {
  Step "NSIS installer (tauri-cli)"
  if (-not (Get-Command cargo-tauri -ErrorAction SilentlyContinue)) {
    cargo install tauri-cli --version "^2" --locked; Check
  }
  Push-Location app
  cargo tauri build --bundles nsis; $code = $LASTEXITCODE
  Pop-Location
  if ($code -ne 0) { throw "tauri build failed" }
  Get-ChildItem target\release\bundle\nsis\*.exe | ForEach-Object {
    Copy-Item $_.FullName "dist\open-rzr-$ver-windows-x64-setup.exe" -Force
    Write-Host "Installer: dist\open-rzr-$ver-windows-x64-setup.exe"
  }
}

# Collect the list first: hashing while Set-Content writes SHA256SUMS.txt in the same pipeline locks the file.
$files = @(Get-ChildItem dist -File | Where-Object { $_.Name -ne "SHA256SUMS.txt" })
$sums = $files | ForEach-Object { "{0}  {1}" -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower(), $_.Name }
$sums | Set-Content -Encoding ascii dist\SHA256SUMS.txt
$sums
