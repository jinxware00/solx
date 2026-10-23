const vscode = require('vscode');
const { completions, hover } = require('./language.cjs');
const { createLaunch, startProcess } = require('./runner.cjs');
const { registerDiagnostics } = require('./diagnostics.cjs');
let diagnostics;

function activate(context) {
  const output = vscode.window.createOutputChannel('Solx');
  diagnostics = registerDiagnostics(vscode, output);
  context.subscriptions.push(diagnostics);
  let running = false;
  let stopRequested = false;
  let process;
  const settings = document => vscode.workspace.getConfiguration('solx', document.uri);
  const range = (document, item) => new vscode.Range(document.positionAt(item.start), document.positionAt(item.end));
  const docs = item => {
    const result = new vscode.MarkdownString();
    if (item.signature) result.appendCodeblock(item.signature, 'text');
    result.appendText(item.description);
    return result;
  };
  const fail = async message => {
    output.appendLine(message);
    output.show(true);
    void vscode.window.showErrorMessage(message);
    return { status: 'failed', message };
  };
  async function run() {
    if (!vscode.workspace.isTrusted) return fail('Trust this workspace before running Solx files.');
    if (running) return fail('A Solx file is already running. Use Solx: Stop Running File first.');
    const document = vscode.window.activeTextEditor?.document;
    if (!document || document.languageId !== 'solx') return fail('Open a Solx file before running it.');
    if (document.isUntitled || document.uri.scheme !== 'file') return fail('Save this document to a local .solx file before running it.');
    running = true;
    stopRequested = false;
    await vscode.commands.executeCommand('setContext', 'solx.running', true);
    try {
      if (document.isDirty) {
        const action = await vscode.window.showWarningMessage('Save changes before running this Solx file?', 'Save and Run', 'Cancel');
        if (action !== 'Save and Run' || !await document.save()) return { status: 'cancelled' };
      }
      if (stopRequested) return { status: 'cancelled' };
      const version = document.version;
      const config = settings(document);
      const launch = await createLaunch({
        file: document.uri.fsPath,
        executablePath: config.get('executablePath', ''),
        cargoManifestPath: config.get('cargoManifestPath', ''),
        workspace: vscode.workspace.getWorkspaceFolder(document.uri)?.uri.fsPath,
      });
      if (stopRequested) return { status: 'cancelled' };
      if (document.isDirty || document.version !== version || document.isClosed) {
        return fail('The document changed while preparing the run. Save it and run again.');
      }
      output.clear();
      output.show(true);
      output.appendLine(`Running ${JSON.stringify(launch.executable)} ${launch.args.map(arg => JSON.stringify(arg)).join(' ')}`);
      output.appendLine(`Working directory: ${launch.cwd}\n`);
      process = startProcess(launch, chunk => output.append(chunk));
      const result = await process.completion;
      if (result.cancelled) { output.appendLine('\n[Stopped]'); return { status: 'cancelled' }; }
      if (result.error) return fail(`Could not launch Solx: ${result.error.message}`);
      output.appendLine(`\n[Exited with ${result.signal ? `signal ${result.signal}` : `code ${result.code}`}]`);
      if (result.code !== 0) {
        void vscode.window.showErrorMessage(`Solx exited with ${result.signal || `code ${result.code}`}. See the Solx output channel.`);
        return { status: 'failed', code: result.code };
      }
      return { status: 'success', code: 0 };
    } catch (error) {
      return fail(`Solx: ${error.message}`);
    } finally {
      process = undefined;
      running = false;
      await vscode.commands.executeCommand('setContext', 'solx.running', false);
    }
  }
  context.subscriptions.push(
    output,
    { dispose: () => process?.stop() },
    vscode.commands.registerCommand('solx.runCurrentFile', run),
    vscode.commands.registerCommand('solx.stopRun', () => { stopRequested = true; process?.stop(); }),
    vscode.languages.registerCompletionItemProvider({ language: 'solx' }, {
      provideCompletionItems(document, position, cancellation) {
        if (cancellation.isCancellationRequested || !settings(document).get('completion.enabled', true)) return [];
        return completions(document.getText(), document.offsetAt(position)).map(item => {
          const result = new vscode.CompletionItem(item.label, vscode.CompletionItemKind[item.kind]);
          result.detail = item.signature || item.detail || 'Solx';
          result.documentation = docs(item);
          result.range = range(document, item);
          result.insertText = item.kind === 'Snippet' ? new vscode.SnippetString(item.insertText) : item.label;
          return result;
        });
      },
    }, '.'),
    vscode.languages.registerHoverProvider({ language: 'solx' }, {
      provideHover(document, position, cancellation) {
        if (cancellation.isCancellationRequested || !settings(document).get('hover.enabled', true)) return undefined;
        const item = hover(document.getText(), document.offsetAt(position));
        return item ? new vscode.Hover(docs(item), range(document, item)) : undefined;
      },
    }),
  );
}

// VS Code awaits deactivate; subscription disposal alone cannot wait for an
// in-flight snapshot write or the checker process to release its input file.
function deactivate() { return diagnostics?.dispose(); }

module.exports = { activate, deactivate };
