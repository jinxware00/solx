const os = require('node:os');
const path = require('node:path');
const { createLaunch } = require('./runner.cjs');
const { startCheck, MAX_SOURCE_BYTES } = require('./checker.cjs');

function registerSymbols(vscode, output, { launch = createLaunch, start = startCheck } = {}) {
  const pending = new Map(), cache = new Map(), active = new Set();
  let disposed = false, disposal;
  const stop = job => {
    if (job.cancelled) return;
    job.cancelled = true;
    try { job.process?.stop(); } catch (error) { output.appendLine(`Could not stop Outline check: ${error.message}`); }
  };
  const cancel = key => { const job = pending.get(key); if (job) stop(job); };
  function consume(job, token) {
    const consumer = {};
    job.consumers.add(consumer);
    let cancellation;
    const cancelled = new Promise(resolve => {
      const release = () => {
        job.consumers.delete(consumer);
        if (!job.consumers.size) stop(job);
        resolve([]);
      };
      cancellation = token.onCancellationRequested(release);
      if (token.isCancellationRequested) release();
    });
    return Promise.race([job.result, cancelled]).then(items => token.isCancellationRequested ? [] : items)
      .finally(() => { cancellation.dispose(); job.consumers.delete(consumer); });
  }
  async function provideDocumentSymbols(document, token) {
    const key = document.uri.toString();
    const settings = vscode.workspace.getConfiguration('solx', document.uri);
    if (disposed || token.isCancellationRequested || document.isClosed || !vscode.workspace.isTrusted
      || !['file', 'untitled'].includes(document.uri.scheme) || !settings.get('symbols.enabled', true)) return [];
    const source = document.getText();
    if (Buffer.byteLength(source, 'utf8') > MAX_SOURCE_BYTES) return [];
    const version = document.version;
    const configuration = JSON.stringify([settings.get('executablePath', ''), settings.get('cargoManifestPath', '')]);
    const previous = cache.get(key);
    if (previous?.version === version && previous.configuration === configuration) return previous.items;
    const shared = pending.get(key);
    if (shared && !shared.cancelled && shared.version === version && shared.configuration === configuration) {
      return consume(shared, token);
    }
    cancel(key);
    let finish;
    const job = { cancelled: false, version, configuration, consumers: new Set(),
      finished: new Promise(resolve => { finish = resolve; }) };
    active.add(job);
    pending.set(key, job);
    const current = () => !disposed && !job.cancelled && !document.isClosed
      && document.version === version && pending.get(key) === job;
    // Outline, breadcrumbs, and symbol commands can request the same version
    // concurrently. Share its checker; one caller's cancellation must not
    // erase another caller's result or kill a process that it still needs.
    job.result = Promise.resolve().then(async () => {
      try {
        if (!current()) return [];
        const folder = vscode.workspace.getWorkspaceFolder(document.uri)
          || (vscode.workspace.workspaceFolders?.length === 1 ? vscode.workspace.workspaceFolders[0] : undefined);
        const workspace = folder?.uri.fsPath;
        const file = document.uri.scheme === 'file' ? document.uri.fsPath : path.join(workspace || os.tmpdir(), 'untitled.solx');
        const command = await launch({ file, workspace, check: true, symbols: true,
          executablePath: settings.get('executablePath', ''), cargoManifestPath: settings.get('cargoManifestPath', '') });
        if (!current()) return [];
        const configured = settings.get('diagnostics.timeoutMs', 30000);
        job.process = start(command, source, { symbols: true,
          timeoutMs: Number.isFinite(configured) ? Math.min(120000, Math.max(1000, configured)) : 30000,
          onCleanupError: error => output.appendLine(error.message) });
        const result = await job.process.completion;
        if (!current() || result.cancelled) return [];
        const kinds = { function: 'Function', variable: 'Variable', constant: 'Constant', module: 'Module' };
        const items = result.symbols.map(item => new vscode.DocumentSymbol(item.name, '', vscode.SymbolKind[kinds[item.kind]],
          new vscode.Range(document.positionAt(item.start), document.positionAt(item.end)),
          new vscode.Range(document.positionAt(item.selectionStart), document.positionAt(item.selectionEnd))));
        cache.set(key, { version, configuration, items });
        return items;
      } catch (error) {
        if (current()) output.appendLine(`Solx Outline unavailable: ${error.message}`);
        return [];
      } finally {
        await job.process?.cleanup;
        if (pending.get(key) === job) pending.delete(key);
        active.delete(job);
        finish();
      }
    });
    return consume(job, token);
  }
  const register = () => vscode.languages.registerDocumentSymbolProvider({ language: 'solx' }, { provideDocumentSymbols });
  let provider = register();
  const refresh = () => {
    for (const key of pending.keys()) cancel(key);
    cache.clear();
    // VS Code caches Outline models by document version and provider identity.
    // Re-register so configuration/trust changes invalidate the editor's cache.
    provider.dispose();
    provider = register();
  };
  const subscriptions = [
    { dispose() { provider.dispose(); } },
    vscode.workspace.onDidCloseTextDocument(document => { const key = document.uri.toString(); cancel(key); cache.delete(key); }),
    vscode.workspace.onDidChangeConfiguration(event => {
      if (!['symbols.enabled', 'executablePath', 'cargoManifestPath'].some(setting => event.affectsConfiguration(`solx.${setting}`))) return;
      refresh();
    }),
    vscode.workspace.onDidGrantWorkspaceTrust(refresh),
  ];
  return { provideDocumentSymbols, dispose() {
    if (disposed) return disposal;
    disposed = true;
    const completions = [...active].map(job => job.finished);
    for (const job of active) stop(job);
    subscriptions.forEach(subscription => subscription.dispose());
    cache.clear();
    disposal = Promise.allSettled(completions).then(() => {});
    return disposal;
  } };
}

module.exports = { registerSymbols };
