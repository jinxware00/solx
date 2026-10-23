const { test } = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const { registerDiagnostics } = require('../src/diagnostics.cjs');

const tick = () => new Promise(resolve => setImmediate(resolve));
function host(overrides = {}) {
  const listeners = {}, published = new Map(), commands = new Map(), requests = [], launched = [];
  const settings = { 'diagnostics.enabled': true, 'diagnostics.delayMs': 100, ...overrides.settings };
  const makeDocument = (name, text) => ({ languageId: 'solx', isClosed: false, isDirty: true, version: 1, text,
    uri: { scheme: 'file', fsPath: path.resolve(name), toString() { return `file:${this.fsPath}`; } },
    getText() { return this.text; }, positionAt(offset) { return offset; } });
  const doc = makeDocument('buffer.solx', 'var x=)');
  const event = name => fn => { listeners[name] = fn; return { dispose() { delete listeners[name]; } }; };
  const status = { show() {}, hide() {}, dispose() {} };
  const output = { lines: [], appendLine(line) { this.lines.push(line); } };
  const vscode = {
    Range: class { constructor(start, end) { this.start = start; this.end = end; } },
    Diagnostic: class { constructor(range, message, severity) { Object.assign(this, { range, message, severity }); } },
    DiagnosticSeverity: { Error: 0 }, StatusBarAlignment: { Left: 1 },
    languages: { createDiagnosticCollection: () => ({ set(uri, items) { published.set(uri.toString(), items); }, delete(uri) { published.delete(uri.toString()); }, dispose() { published.clear(); } }) },
    commands: { registerCommand(name, action) { commands.set(name, action); return { dispose() {} }; } },
    window: { activeTextEditor: { document: doc }, createStatusBarItem: () => status, onDidChangeActiveTextEditor: event('active') },
    workspace: { isTrusted: overrides.trusted !== false, textDocuments: [],
      getConfiguration: () => ({ get: (name, fallback) => settings[name] ?? fallback }),
      getWorkspaceFolder: () => ({ uri: { fsPath: path.resolve('workspace') } }),
      onDidOpenTextDocument: event('open'), onDidChangeTextDocument: event('change'), onDidCloseTextDocument: event('close'),
      onDidChangeConfiguration: event('config'), onDidGrantWorkspaceTrust: event('trust') },
  };
  const controller = registerDiagnostics(vscode, output, {
    launch: async options => { launched.push(options); if (overrides.launch) return overrides.launch(options); return {}; },
    start: (_launch, source, options) => {
      let finish, fail;
      const request = { source, completion: new Promise((resolve, reject) => { finish = resolve; fail = reject; }),
        finish: value => finish(value), fail: error => fail(error), cleanupError: options.onCleanupError,
        stopped: false, stop() { this.stopped = true; } };
      requests.push(request); return request;
    },
  });
  const edit = text => { doc.text = text; doc.version++; listeners.change({ document: doc }); };
  const result = () => published.get(doc.uri.toString());
  return { controller, doc, edit, makeDocument, vscode, listeners, published, requests, launched, settings, status, output, result, commands };
}
const invalid = { diagnostics: [{ code: 'parse-error', message: 'expected expression', start: 6, end: 7 }] };

test('diagnostics use dirty buffers, debounce rapid edits, and clear stale ranges immediately', async t => {
  const h = host(); t.after(() => h.controller.dispose());
  h.listeners.open(h.doc);
  h.edit('var x=1'); h.edit('var x=)');
  assert.equal(h.launched.length, 0);
  await new Promise(resolve => setTimeout(resolve, 150));
  assert.equal(h.requests.length, 1);
  assert.equal(h.requests[0].source, 'var x=)');
  h.requests[0].finish(invalid); await tick();
  assert.equal(h.result()[0].range.start, 6);
  assert.equal(h.result()[0].source, 'Solx');
  h.edit('var x=1');
  assert.equal(h.result(), undefined);
});

test('late results are ignored and only one process runs across documents', async t => {
  const h = host(); t.after(() => h.controller.dispose());
  const first = h.controller.request(h.doc, true); await tick();
  h.doc.text = 'var x=1'; h.doc.version++;
  const newer = h.controller.request(h.doc, true);
  const other = h.makeDocument('other.solx', 'var y=1');
  const second = h.controller.request(other, true);
  assert.equal(h.requests.length, 1); assert.ok(h.requests[0].stopped);
  assert.equal((await first).status, 'cancelled');
  h.requests[0].finish(invalid); await tick();
  assert.equal(h.result(), undefined); assert.equal(h.requests.length, 2);
  h.requests[1].finish({ diagnostics: [] }); await tick();
  assert.equal((await newer).status, 'checked'); assert.equal(h.requests.length, 3);
  h.requests[2].finish({ diagnostics: [] });
  assert.equal((await second).status, 'checked');
});

test('close, disable and dispose cancel work and reject results without phantom errors', async () => {
  for (const action of ['close', 'disable', 'dispose']) {
    const h = host();
    const pending = h.controller.request(h.doc, true); await tick();
    if (action === 'close') { h.doc.isClosed = true; h.listeners.close(h.doc); }
    if (action === 'disable') { h.vscode.workspace.textDocuments = [h.doc]; h.settings['diagnostics.enabled'] = false; h.listeners.config({ affectsConfiguration: () => true }); }
    if (action === 'dispose') h.controller.dispose();
    assert.ok(h.requests[0].stopped); h.requests[0].finish(invalid);
    assert.equal((await pending).status, 'cancelled'); await tick();
    assert.equal(h.result(), undefined); h.controller.dispose();
  }
});

test('untrusted, virtual and large buffers never launch; untitled buffers are supported', async t => {
  const h = host({ trusted: false }); t.after(() => h.controller.dispose());
  assert.equal((await h.controller.request(h.doc, true)).status, 'skipped');
  assert.equal(h.launched.length, 0);
  h.vscode.workspace.isTrusted = true;
  h.doc.uri.scheme = 'git';
  assert.equal((await h.controller.request(h.doc, true)).status, 'skipped');
  h.doc.uri.scheme = 'untitled'; h.doc.text = ' '.repeat(1_048_577);
  assert.equal((await h.controller.request(h.doc, true)).status, 'skipped');
  h.doc.text = 'var x=1';
  const pending = h.controller.request(h.doc, true); await tick();
  assert.equal(h.launched.length, 1); assert.equal(h.launched[0].check, true);
  h.requests[0].finish({ diagnostics: [] });
  assert.equal((await pending).status, 'checked');
});

test('configuration changes during preparation prevent launching obsolete settings', async t => {
  let ready;
  const preparation = new Promise(resolve => { ready = resolve; });
  const h = host({ launch: () => preparation }); t.after(() => h.controller.dispose());
  h.vscode.workspace.textDocuments = [h.doc];
  const old = h.controller.request(h.doc, true); await tick();
  h.settings['diagnostics.enabled'] = false;
  h.listeners.config({ affectsConfiguration: () => true });
  ready({}); await tick();
  assert.equal((await old).status, 'cancelled'); assert.equal(h.requests.length, 0);
});

test('checker failures use status/output rather than fake source diagnostics and can be retried', async t => {
  const h = host(); t.after(() => h.controller.dispose());
  for (let n = 0; n < 2; n++) {
    const pending = h.commands.get('solx.checkCurrentFile')(); await tick();
    h.requests[n].fail(new Error('Executable not found'));
    assert.equal((await pending).status, 'failed'); assert.equal(h.result(), undefined);
    assert.match(h.status.text, /unavailable/);
  }
  assert.equal(h.output.lines.length, 1);
  const pending = h.controller.request(h.doc, true); await tick();
  h.requests[2].finish({ diagnostics: [] });
  assert.equal((await pending).status, 'checked'); assert.deepEqual(h.result(), []);
});

test('cleanup failures from obsolete checks remain visible without stale source diagnostics', async t => {
  const h = host(); t.after(() => h.controller.dispose());
  const pending = h.controller.request(h.doc, true); await tick();
  h.edit('var x=1');
  h.requests[0].cleanupError(new Error('Could not remove snapshot at temporary-path'));
  h.requests[0].finish(invalid);
  assert.equal((await pending).status, 'cancelled'); await tick();
  assert.equal(h.result(), undefined);
  assert.ok(h.output.lines.some(line => line.includes('temporary-path')));
});

test('disposal waits for active checker cleanup and remains idempotent', async () => {
  const h = host();
  const pending = h.controller.request(h.doc, true); await tick();
  let finished = false;
  const disposal = h.controller.dispose();
  assert.equal(h.controller.dispose(), disposal);
  disposal.then(() => { finished = true; });
  await tick();
  assert.equal(finished, false);
  assert.equal((await pending).status, 'cancelled');
  h.requests[0].finish({ cancelled: true });
  await disposal;
  assert.equal(finished, true);
  assert.equal(h.result(), undefined);
});
