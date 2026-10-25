# Installing Solx

Solx is in early development. Check [GitHub Releases](https://github.com/jinxware00/solx/releases)
for packaged downloads, or build from this repository. The extension can be
installed from its VSIX without a Marketplace listing.

## Windows terminal installer

Download or build and extract `solx-0.2.0-windows-x64.zip`. Open PowerShell or Windows Terminal in
the extracted folder and run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1
```

This starts a terminal installer with a Solx ASCII header, five numbered
stages, and coloured status messages. It uses your terminal's background and
palette. `-NoColor`, `NO_COLOR`, `TERM=dumb`, and redirected output select plain
text. It opens no graphical installer. The command's execution-policy setting
applies only to this process; it does not change your machine's policy.

Requires Windows x64 and Windows PowerShell 5.1 (included in Windows 10/11).
No administrator privileges, internet connection, Rust, or npm are needed.
The default installation is `%LOCALAPPDATA%\Solx`; enter another absolute
folder when prompted. Prompts show their default in brackets. Enter `q` to
cancel, or decline the final confirmation before any installation changes.
Ctrl+C interrupts the installer; unfinished file replacement is rolled back.
Already completed stages remain installed and can be finished by rerunning it.

The installer asks whether to add `bin` to your **user PATH** and whether to
**Install Solx for Visual Studio Code**. Editor integration defaults to yes
when desktop VS Code 1.85+ and its CLI are found. Decline to install only Solx.
Missing or unsupported VS Code is explained and does not prevent installation.
Standard per-user/system locations, App Paths, and absolute PATH entries for
`code.cmd` are checked. Pass an absolute path such as `-CodeRoot 'C:\Tools\Microsoft VS Code'` for a
custom installation. Insiders, VSCodium, and remote extension hosts are not
automatically detected.

A current source build bundles Sol Language Support **0.1.0**, identified as
`jinxware00.solx-ls`, with no installation-time downloads. The VSIX is retained
under `<installation folder>\editor` even if
skipped. If the CLI fails or times out, Solx stays installed and the terminal
prints recovery instructions. Read `editor-install-result.txt` and, when
produced, `vscode-install.log` in that folder. Install manually using
**Extensions: Install from VSIX...** in VS Code, or:

```powershell
code --install-extension 'path\to\solx-ls-0.1.0.vsix'
```

The extension was renamed from `jinxware00.solx-language-support` and starts at
version 0.1.0 under its new identity. If the earlier extension is installed,
disable or uninstall it in VS Code before enabling `jinxware00.solx-ls` to avoid
duplicate diagnostics and commands. The installer leaves that separate extension
in place. The interpreter remains version 0.2.0.

Restart VS Code and open a new terminal after PATH changes:

```sh
solx --version
solx path/to/program.solx
solx
```

Open a `.solx` file in VS Code and choose **Solx: Run Current File** or the play
button. Save changes when prompted; output and errors appear in the Solx output
channel. If executable discovery fails, set `solx.executablePath` to the
installed `bin\solx.exe`, without surrounding quotes. See the
[extension README](../vscode-solx/README.md) for editor settings and limitations.

## Automation, upgrades, and removal

For unattended installation, `-Yes` accepts the destination and adds user PATH,
but **skips the extension unless explicitly selected**:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1 -Yes -VSCode Yes
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1 -Yes -VSCode No -AddToPath No -InstallDirectory 'C:\Tools\Solx'
```

Exit codes: `0` means interpreter installation completed (optional setup may
have warnings); `1` means an installation error; `130` means prompt cancellation.
For automation, inspect `editor-install-result.txt` for extension status.
`-ExtensionsDirectory` and `-UserDataDirectory` can isolate the VS Code CLI for
testing. The installer validates the bundle checksums before changing files.

Close running Solx programs before upgrading and reuse the same folder. Replaced
files are backed up under `.solx-backups` inside that folder. User programs are
left in place. Interrupted file replacement restores the previous files.
Backups are retained for manual recovery; remove them yourself when no longer
needed. Install paths containing semicolons, drive roots, junctions, or symbolic
links are rejected. Ordinary OneDrive cloud folders are supported.

To uninstall, run from the installation folder:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\uninstall.ps1
```

The terminal uninstaller removes only unchanged files recorded by this installer
and the PATH entry only if it added it. Modified files, backups, user programs,
logs, and the VS Code extension remain. Manage the extension in VS Code.
No Start menu shortcuts or Windows Installed apps entries are created.

## Build from source

Install current stable Rust with Cargo. Windows builds use the MSVC toolchain,
Visual Studio C++ Build Tools, and a Windows SDK. From this repository:

```sh
cargo build --release --locked
cargo run --locked -- examples/functions.solx
cargo install --path . --locked
```

`cargo install` places Solx in Cargo's bin directory; it does not install the
extension. Developers can set `solx.cargoManifestPath` to this repository's
`Cargo.toml` so VS Code rebuilds before running changed interpreter code.
See [development and packaging](DEVELOPMENT.md) for bundle build instructions.
