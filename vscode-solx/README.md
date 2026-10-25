<img src="images/icon.png" alt="Solx" width="96" height="96">

# Sol Language Support

Write, check, and run `.solx` files in Visual Studio Code. Syntax highlighting,
live diagnostics, Outline symbols, completions, construct snippets, and API documentation follow the existing
[Solx implementation and specification](https://github.com/jinxware00/solx/blob/main/docs/SPEC.md).

## Features

- **Run Current File** with streamed output, error reporting, and a stop command.
- **Live syntax diagnostics** from the real lexer/parser, including unsaved and
  untitled buffers, without executing Solx code or loading imports.
- **Outline and Go to Symbol** for parser-derived functions, variables, constants,
  and imports. Optional conservative undefined-name checks use the same parser.
- **Completions** for implemented keywords, `print`, `str`, `num`, variable
  annotations, and explicitly imported standard modules, including aliases.
- **Snippets** for named/anonymous functions, conditionals, numeric/collection/while
  loops, `try`/`catch`, imports, and variable declarations. Trigger suggestions with Ctrl+Space.
- **Hover documentation** for recognized built-ins and the `core`, `math`,
  `string`, `list`, `object`, `json`, `fs`, `path`, and `time` APIs, including signatures and constraints.
- **Syntax highlighting**, `--` line comments, bracket matching, and automatic
  bracket/quote closing and surrounding.

```solx
import "string"

fn greet(name)
    return "Hello, " .. string.trim(name) .. "!"
end

var names = [" Jinx ", "Ada"]
for name in names do
    print(greet(name))
end
```

## Install locally

Current source adds editor support for `fs`, `path`, and `time`. These APIs and
their editor additions are not in the v0.2.0 / extension 0.1.0 release downloads.
Use matching current sources. File and clock programs need explicit CLI grants;
run them from the terminal, for example
`solx --allow-read --allow-write --allow-time --fs-root . program.solx`.
The Run command uses default grants. See
[host libraries](https://github.com/jinxware00/solx/blob/main/docs/HOST_LIBRARIES.md).

Requires VS Code 1.85 or later. This extension has not been published to the
Marketplace. Download `solx-ls-0.1.0.vsix` from the
[GitHub releases](https://github.com/jinxware00/solx/releases), then use
**Extensions: Install from VSIX...**, or:

```sh
code --install-extension solx-ls-0.1.0.vsix
```

The extension identifier is `jinxware00.solx-ls`. Version 0.1.0 begins this new
package identity and retains the current editor features. If you installed
the older `solx` or `jinxware00.solx-language-support` extension, uninstall that
copy before installing Sol Language Support to avoid duplicate language providers. The
language ID and settings prefix remain `solx`.

The Windows terminal installer can bundle the extension. Its **Install Solx for Visual
Studio Code** prompt defaults to yes when supported VS Code is found.
You can decline and install the VSIX later. Restart VS Code after installing
Solx so it receives the updated PATH. See the repository's
[installation guide](https://github.com/jinxware00/solx/blob/main/docs/INSTALLING.md).
Use a current source build of Solx 0.2 with this extension. Earlier installer
ZIPs contain older binaries and VSIX files; a standalone extension update does
not update those bundles or an installed interpreter.

## Configure the interpreter

Highlighting, completion, snippets, and hover work without installing Solx.
Running, diagnostics, and Outline require a trusted workspace and one of these
interpreter setups:

| Setup | Setting | Behavior |
| --- | --- | --- |
| Installed Solx on PATH | Leave both path settings empty | Search the extension host's PATH for `solx` (`solx.exe` on Windows). |
| Installed Solx elsewhere | `solx.executablePath` | Use the specified executable or command name. |
| Developing the Solx interpreter | `solx.cargoManifestPath` | Use Cargo to rebuild the source checkout before each run or syntax check. |

For an installed Windows executable, a settings example is:

```json
{
  "solx.executablePath": "C:\\Tools\\Solx\\solx.exe"
}
```

For a workspace opened at the Solx repository root:

```json
{
  "solx.cargoManifestPath": "Cargo.toml"
}
```

Use only one path setting. Paths may be absolute or relative to the active
file's workspace folder. Untitled buffers use the sole workspace folder if
there is one; use absolute settings in an ambiguous multi-root workspace.
Spaces need no additional quotes. Do not put flags,
shell commands, or environment-variable expressions in a path setting.
Windows executables must be `.exe` files, not `.cmd`/`.bat` wrappers.

PATH lookup skips empty and relative entries; it never implicitly searches
the current directory. Restart VS Code after changing PATH. In a remote
workspace, the executable must be available on the extension host's machine.

Cargo mode requires Cargo on PATH and the Solx checkout's normal Rust build
requirements. It runs `cargo run --quiet --locked --manifest-path <manifest>
--bin solx -- <file>`. The extension refuses direct `target/.../solx`
binaries, including symlinks to them, because they may be stale. Installed
binaries outside a build tree are maintained by the user.

## Run a file

1. Open a saved `.solx` file.
2. Select **Solx: Run Current File** from the Command Palette or the editor's
   play button. If the file has unsaved changes, choose **Save and Run** or cancel.
3. Read stdout, stderr, and the exit code in the **Solx** output channel.
4. Use **Solx: Stop Running File** or the stop button to terminate a long run.

Execution uses the file's directory as the working directory and passes the
absolute file path as one argument without a shell. Untitled and virtual
documents must be saved first. Only one run is active at a time. Output is
limited to two million characters per run; additional output is drained and
discarded. This is a noninteractive runner, so stdin is closed.

The bundled `examples/hello.solx` remains runnable from the repository root:

```sh
cargo run -- vscode-solx/examples/hello.solx
```

## Completion, hover, diagnostics, and Outline

Type `import "math"`, then `math.` to see the documented members. An alias
such as `import "math" as m` enables `m.` too. Arbitrary object members and
unimported modules do not receive standard-library suggestions. Suggestions
and hover from Solx are suppressed inside strings and comments; VS Code's
own word suggestions still follow your editor settings. Set
`solx.completion.enabled` or `solx.hover.enabled` to `false` to disable them.

API recognition uses direct lexical bindings, including ordinary declarations,
parameters, and imports. It is a reference aid, not type inference or a
language server. Assignments invalidate inferred APIs. Conditional execution,
closures called after later rebinding, arbitrary module-valued expressions,
and file exports are not analyzed. Requests for documents over one million
UTF-16 characters are skipped to keep the editor responsive.

Live diagnostics use the interpreter's parse-only `check --json` interface.
The exact current buffer is checked after 350 ms without edits; saving is
unnecessary. The first lexer or parser error appears in the
Problems panel and editor. A valid buffer clears the error. UTF-8 byte ranges
are converted to VS Code's UTF-16 offsets, including emoji and CRLF text.

On Windows, each check writes the buffer (including unsaved text) to a newly
created directory under your user temporary directory, then runs
`solx check --json -- <snapshot>`. This avoids the roughly 26-second startup
stall measured with stdin checks on the development machine. Snapshots use
exclusive file creation and the temporary directory's Windows access controls.
They are removed after completion, errors, cancellation, or timeouts; normal
extension shutdown waits for pending cleanup. Cleanup
failures are reported with the path; a crash or OS access denial can leave a
snapshot behind. Disable both `solx.diagnostics.enabled` and `solx.symbols.enabled` if you do not want unsaved
buffers written to temporary files. Other platforms use `check --json --stdin`
without creating snapshots. No buffer content is embedded in command arguments.

Use **Solx: Check Current File** or click the Solx status-bar item to check
immediately. The status bar distinguishes checking, syntax OK, errors, and an
unavailable checker. Missing executables, older CLIs without the check protocol,
Cargo build failures, and timeouts are explained there and in the Solx output
channel; they are not fabricated source diagnostics. Update/rebuild the
interpreter if its protocol is missing. There is no fallback to executing code.

Diagnostic checks run one at a time. Outline requests may run alongside them.
New edits, closing a document, disabling diagnostics,
and changing interpreter settings cancel obsolete work. Late results never
overwrite diagnostics for newer text. Only trusted local-file and untitled
buffers up to 1 MiB of UTF-8 source are checked. Cargo mode rebuilds as needed
using `cargo run ... -- check --json` with the appropriate buffer input; the
configured Rust checkout must be trusted, including its build scripts.

| Setting | Default | Purpose |
| --- | --- | --- |
| `solx.diagnostics.enabled` | `true` | Enable automatic and manual syntax checking. |
| `solx.diagnostics.checkNames` | `false` | Also check for definitely unbound names. Requires Solx 0.2. |
| `solx.symbols.enabled` | `true` | Provide parser-derived Outline and Go to Symbol entries. Requires Solx 0.2. |
| `solx.diagnostics.delayMs` | `350` | Debounce edits, from 100 to 5,000 ms. |
| `solx.diagnostics.timeoutMs` | `30000` | Check timeout, from 1,000 to 120,000 ms; increase for cold Cargo builds. |

Default checking covers syntax, with one error at a time. Enable
`solx.diagnostics.checkNames` to report up to 100 unbound names after syntax
passes. The analysis predeclares block locals and file globals, supports
parameters and loop/catch scopes, and admits the implicit `exports` binding.
It can miss use-before-definition and conditional declaration problems; it
does not infer values, validate signatures/types, load imports, or detect infinite loops.
Runtime errors still appear when you explicitly run a file. The versioned
CLI schema is documented in the repository's
[checking guide](https://github.com/jinxware00/solx/blob/main/docs/CHECKING.md).

The `try` snippet creates a handler that prints `err.message`. Catch bindings
and the separate try/catch scopes are respected when recognizing APIs. These
keywords require a current interpreter. JSON completion/hover and the updated
nil/number reference target Solx 0.2; use Cargo mode for source development.

Outline uses `check --symbols --json` and shows declaration names and exact
source selections. It works with unsaved buffers, caches by document version,
and suppresses stale/cancelled results. Syntax-invalid buffers have no symbols.
It is a flat declaration list; references, rename, and cross-file navigation
are not implemented.

Raw multiline strings use `"""text"""`, preserve newlines and backslashes,
and suppress completions/hover throughout their content. Interpolation remains
unimplemented: braces are literal text. Use concatenation (`..`) and `~=` for inequality. No formatting, debugger, or custom block
indentation/folding is provided. Colors depend on your theme.

## Develop, test, and package

Open **the `vscode-solx` directory** in VS Code and press **F5** to launch
**Preview Solx Extension** with the bundled example. No compilation or npm
installation is needed to use the extension's runtime code.

Development tooling requires Node.js 22 or later and npm. From `vscode-solx`:

```sh
npm ci
npm test
npm run test:interpreter
npm run test:host
npm run package:files
npm run package
npm run test:package -- 1.85.2
```

- `npm test` checks TextMate scopes, completions, hover, runner behavior,
  diagnostics/symbols/protocol/snapshot lifecycle handling, metadata, and local assets.
  It runs before packaging.
- `test:interpreter` builds the existing CLI with Cargo in `.test-artifacts`
  and runs valid/invalid programs, snippet defaults, and non-executing buffer
  checks with real byte ranges. It runs no Rust tests.
- `test:host` launches an isolated VS Code Extension Development Host and
  exercises the real providers, live Problems entries, unsaved/untitled
  diagnostics, Outline, name checks, raw strings, and run command. It requires desktop VS Code;
  set `VSCODE_EXECUTABLE` to its executable if discovery fails. Results are
  written under `.test-artifacts`. This opens a temporary editor window.
  To download and test an isolated official build instead, use
  `npm run test:host -- 1.85.2` (network access required on the first run).
- `test:package` installs the current VSIX into an isolated extension directory,
  checks every shipped file, and runs the host tests against that installed
  copy. It also checks that no snapshots remain in the isolated temporary
  directory after shutdown. It accepts the same optional VS Code version as `test:host`.
- `npm run icon` uses Windows PowerShell and System.Drawing to resize the
  official `../assets/logo.png` into a transparent 512 x 512 `images/icon.png`.
  It preserves aspect ratio and the complete artwork, with clear space around
  it. Both original logo files are preserved. Regeneration requires Windows;
  the resulting icon is checked in for development on other platforms.

The VSIX contains the runtime JavaScript, snippet data, icon, grammar, editing
configuration, example, README, changelog, and MIT license. It has no runtime
npm dependencies. Development tools and test artifacts are excluded.

## Marketplace preparation

The proposed slug and display name are configured locally; their Marketplace
availability has not been verified. Publisher `jinxware00` and the
[author profile](https://github.com/jinxware00) are configured as supplied.
Before publishing manually, verify publisher ownership, name/identifier
availability, repository links, and supported VS Code versions. Packaging
does not publish, create accounts, or request credentials.

[Repository](https://github.com/jinxware00/solx) ·
[Language specification](https://github.com/jinxware00/solx/blob/main/docs/SPEC.md) ·
[Report an issue](https://github.com/jinxware00/solx/issues)
