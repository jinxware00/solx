$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$output = Join-Path $root 'dist'
$payload = Join-Path $PSScriptRoot ('.artifacts/bundle-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $payload,$output,(Join-Path $payload 'bin'),(Join-Path $payload 'editor') | Out-Null
$package = Get-Content (Join-Path $root 'vscode-solx/package.json') -Raw | ConvertFrom-Json
$version = [regex]::Match((Get-Content (Join-Path $root 'Cargo.toml') -Raw), '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Cannot read Solx version.' }
$vsixName = "$($package.name)-$($package.version).vsix"
$vsix = Join-Path $root "vscode-solx/$vsixName"
$receiptPath = Join-Path $root 'vscode-solx/.test-artifacts/package-validation.json'
if (-not (Test-Path $receiptPath)) { throw 'Build the VSIX and run npm run test:package before building the installer.' }
$receipt = Get-Content $receiptPath -Raw | ConvertFrom-Json
if (-not $receipt.success -or $receipt.sha256 -ne (Get-FileHash $vsix).Hash) { throw 'The current VSIX has not passed packaged host validation.' }
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::OpenRead($vsix)
try {
    $reader = New-Object IO.StreamReader ($zip.GetEntry('extension/package.json').Open())
    try { $packaged = $reader.ReadToEnd() | ConvertFrom-Json } finally { $reader.Dispose() }
    if ($packaged.version -ne $package.version -or $packaged.publisher -ne 'jinxware00' -or $packaged.name -ne $package.name) { throw 'VSIX metadata does not match source.' }
} finally { $zip.Dispose() }
& cargo build --release --locked --manifest-path (Join-Path $root 'Cargo.toml') --bin solx --target x86_64-pc-windows-msvc --target-dir (Join-Path $PSScriptRoot '.artifacts/cargo')
if ($LASTEXITCODE -ne 0) { throw 'Cargo release build failed.' }
Copy-Item -LiteralPath (Join-Path $PSScriptRoot '.artifacts/cargo/x86_64-pc-windows-msvc/release/solx.exe') -Destination (Join-Path $payload 'bin/solx.exe')
Copy-Item -LiteralPath $vsix -Destination (Join-Path $payload "editor/$vsixName")
foreach ($file in @('install.ps1','uninstall.ps1','Installer.psm1','VSCode.psm1')) { Copy-Item -LiteralPath (Join-Path $PSScriptRoot $file) -Destination $payload }
Copy-Item -LiteralPath (Join-Path $root 'LICENSE') -Destination $payload
Copy-Item -LiteralPath (Join-Path $root 'docs/INSTALLING.md') -Destination $payload
Copy-Item -LiteralPath (Join-Path $root 'assets/logo.png') -Destination $payload
$files = [ordered]@{}
foreach ($file in @('bin/solx.exe', "editor/$vsixName", 'LICENSE', 'INSTALLING.md', 'uninstall.ps1', 'Installer.psm1')) { $files[$file] = (Get-FileHash -LiteralPath (Join-Path $payload $file)).Hash }
[ordered]@{ name = 'solx'; version = $version; extensionVersion = $package.version; files = $files } | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $payload 'bundle.json') -Encoding UTF8
$archive = Join-Path $output "solx-$version-windows-x64.zip"
$pending = Join-Path $output ('bundle-' + [guid]::NewGuid().ToString('N') + '.zip')
[IO.Compression.ZipFile]::CreateFromDirectory($payload, $pending)
Move-Item -LiteralPath $pending -Destination $archive -Force
$head = $null; $dirty = $null
if (Test-Path -LiteralPath (Join-Path $root '.git')) {
    $head = & git -C $root rev-parse HEAD
    $dirty = @(& git -C $root status --porcelain -- src Cargo.toml Cargo.lock).Count -gt 0
}
[ordered]@{ interpreterVersion = $version; extensionVersion = $package.version; sourceCommit = $head;
    runtimeHasLocalChanges = $dirty; sha256 = (Get-FileHash $archive).Hash; vsixSha256 = (Get-FileHash $vsix).Hash } | ConvertTo-Json | Set-Content ($archive + '.json')
Write-Output "Terminal installer: $archive"
Write-Output 'Extract the ZIP, open a terminal in that folder, then run: powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1'
