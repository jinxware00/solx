const path = require('node:path');
const os = require('node:os');
const { createLaunch } = require('./runner.cjs');
const { MAX_SOURCE_BYTES, startCheck } = require('./checker.cjs');

function registerDiagnostics(vscode, output, { launch = createLaunch, start = startCheck } = {}) {
  const collection = vscode.languages.createDiagnosticCollection('solx');
  const status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 5);
  status.command = 'solx.checkCurrentFile';
  const jobs = new Map(), states = new Map(), reported = new Map();
  let active, disposed = false;
  const key = document => document.uri.toString();
  const config = document => vscode.workspace.getConfiguration('solx', document.uri);
  const number = (value, fallback, minimum, maximum) => Number.isFinite(value) ? Math.min(maximum, Math.max(minimum, value)) : fallback;
  const supported = document => !document.isClosed && document.languageId === 'solx' && ['file', 'untitled'].includes(document.uri.scheme);
  const enabled = document => supported(document) && vscode.workspace.isTrusted && config(document).get('diagnostics.enabled', true);
  const refreshStatus = () => {
    const document = vscode.window.activeTextEditor?.document;
    if (!document || document.languageId !== 'solx') { status.hide(); return; }
    const state = states.get(key(document));
    status.text = state?.text || 'Solx: Check syntax';
    status.tooltip = state?.message || 'Check the current buffer with the Solx lexer and parser. Click to check again.';
    status.show();
  };
  const state = (document, text, message) => { states.set(key(document), { text, message }); refreshStatus(); };
  const current = job => !disposed && jobs.get(key(job.document)) === job && job.document.version === job.version && enabled(job.document);
  const cancel = document => {
    const job = jobs.get(key(document));
    if (!job) return;
    clearTimeout(job.timer);
    jobs.delete(key(document));
    job.resolve({ status: 'cancelled' });
    try { job.process?.stop(); } catch (error) { output.appendLine(`Could not stop syntax check: ${error.message}`); }
  };

  async function drain() {
    if (disposed || active) return;
    const job = [...jobs.values()].find(candidate => candidate.ready);
    if (!job) return;
    active = job;
    const document = job.document;
    try {
      if (!current(job)) return;
      state(document, '$(sync~spin) Solx: Checking', 'Checking syntax without executing the buffer.');
      const folder = vscode.workspace.getWorkspaceFolder(document.uri)
        || (vscode.workspace.workspaceFolders?.length === 1 ? vscode.workspace.workspaceFolders[0] : undefined);
      const workspace = folder?.uri.fsPath;
      // Untitled input needs only a working directory. No temporary source file
      // is created; the exact buffer travels over stdin, never command arguments.
      const file = document.uri.scheme === 'file' ? document.uri.fsPath : path.join(workspace || os.tmpdir(), 'untitled.solx');
      const settings = config(document);
      const command = await launch({ file, workspace, check: true,
        executablePath: settings.get('executablePath', ''), cargoManifestPath: settings.get('cargoManifestPath', '') });
      if (!current(job)) return;
      job.process = start(command, job.source, { timeoutMs: number(settings.get('diagnostics.timeoutMs'), 30000, 1000, 120000) });
      const result = await job.process.completion;
      if (!current(job) || result.cancelled) return;
      const diagnostics = result.diagnostics.map(item => {
        const range = new vscode.Range(document.positionAt(item.start), document.positionAt(item.end));
        const diagnostic = new vscode.Diagnostic(range, item.message, vscode.DiagnosticSeverity.Error);
        diagnostic.source = 'Solx'; diagnostic.code = item.code;
        return diagnostic;
      });
      collection.set(document.uri, diagnostics);
      reported.delete(key(document));
      state(document, diagnostics.length ? `$(error) Solx: ${diagnostics.length} error` : '$(check) Solx: Syntax OK',
        'Lexer/parser check complete. Imports, names, types, and runtime behavior are not checked.');
      job.resolve({ status: 'checked', diagnostics: diagnostics.length });
    } catch (error) {
      if (current(job)) {
        collection.delete(document.uri);
        state(document, '$(warning) Solx: Check unavailable', error.message);
        if (reported.get(key(document)) !== error.message) {
          output.appendLine(`Syntax check: ${error.message}`);
          reported.set(key(document), error.message);
        }
        job.resolve({ status: 'failed', message: error.message });
      }
    } finally {
      if (jobs.get(key(document)) === job) jobs.delete(key(document));
      job.resolve({ status: 'cancelled' });
      active = undefined;
      void drain();
    }
  }

  function request(document, immediate = false) {
    if (disposed || !document) return Promise.resolve({ status: 'skipped' });
    cancel(document);
    collection.delete(document.uri); // Never leave an old range on changed text.
    if (!enabled(document)) {
      const message = !vscode.workspace.isTrusted ? 'Trust the workspace to launch the configured checker.'
        : !supported(document) ? 'Checking supports local files and untitled Solx buffers.' : 'Enable solx.diagnostics.enabled to check syntax.';
      state(document, 'Solx: Check disabled', message);
      return Promise.resolve({ status: 'skipped' });
    }
    const source = document.getText();
    if (Buffer.byteLength(source, 'utf8') > MAX_SOURCE_BYTES) {
      state(document, '$(warning) Solx: File too large', 'Syntax checking is limited to 1 MiB of UTF-8 source.');
      return Promise.resolve({ status: 'skipped' });
    }
    let resolve;
    const completion = new Promise(done => { resolve = done; });
    const job = { document, version: document.version, source, resolve, ready: immediate };
    jobs.set(key(document), job);
    state(document, 'Solx: Check pending', 'Waiting for edits to settle.');
    if (immediate) void drain();
    else job.timer = setTimeout(() => {
      job.ready = true; void drain();
    }, number(config(document).get('diagnostics.delayMs'), 350, 100, 5000));
    return completion;
  }

  const refresh = () => {
    for (const document of vscode.workspace.textDocuments) if (document.languageId === 'solx') void request(document);
    refreshStatus();
  };
  const subscriptions = [
    collection, status,
    vscode.commands.registerCommand('solx.checkCurrentFile', () => request(vscode.window.activeTextEditor?.document, true)),
    vscode.workspace.onDidOpenTextDocument(document => { if (document.languageId === 'solx') void request(document); }),
    vscode.workspace.onDidChangeTextDocument(event => { if (event.document.languageId === 'solx' || jobs.has(key(event.document))) void request(event.document); }),
    vscode.workspace.onDidCloseTextDocument(document => {
      cancel(document); collection.delete(document.uri); states.delete(key(document)); reported.delete(key(document)); refreshStatus();
    }),
    vscode.workspace.onDidChangeConfiguration(event => {
      if (['diagnostics', 'executablePath', 'cargoManifestPath'].some(setting => event.affectsConfiguration(`solx.${setting}`))) refresh();
    }),
    vscode.workspace.onDidGrantWorkspaceTrust(refresh),
    vscode.window.onDidChangeActiveTextEditor(refreshStatus),
  ];
  refresh();
  return { request, dispose() {
    disposed = true;
    for (const job of [...jobs.values()]) cancel(job.document);
    for (const subscription of subscriptions) subscription.dispose();
    states.clear(); reported.clear();
  } };
}

module.exports = { registerDiagnostics };
