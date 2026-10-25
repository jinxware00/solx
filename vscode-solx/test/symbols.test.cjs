const { test } = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const { registerSymbols } = require('../src/symbols.cjs');
const tick = () => new Promise(resolve => setImmediate(resolve));
const deferred = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };

function host(options = {}) {
  const events = {}, requests = [], launches = [], lines = [], registrations = [];
  const settings = { ...options.settings };
  const event = name => callback => { events[name] = callback; return { dispose() {} }; };
  const doc = { version: 1, isClosed: false, text: 'var item=1',
    uri: { scheme: 'file', fsPath: path.resolve('buffer.solx'), toString() { return this.fsPath; } },
    getText() { return this.text; }, positionAt(offset) { return offset; } };
  const token = () => { let listener; return { isCancellationRequested: false,
    onCancellationRequested(fn) { listener = fn; return { dispose() {} }; },
    cancel() { this.isCancellationRequested = true; listener?.(); } }; };
  const vscode = {
    Range: class { constructor(start, end) { Object.assign(this, { start, end }); } },
    DocumentSymbol: class { constructor(name, detail, kind, range, selectionRange) { Object.assign(this, { name, detail, kind, range, selectionRange }); } },
    SymbolKind: { Variable: 1, Function: 2, Constant: 3, Module: 4 },
    languages: { registerDocumentSymbolProvider: (selector, provider) => {
      const registration = { selector, provider, disposed: false, dispose() { this.disposed = true; } };
      registrations.push(registration); return registration;
    } },
    workspace: { isTrusted: options.trusted !== false,
      getConfiguration: () => ({ get: (name, fallback) => settings[name] ?? fallback }),
      getWorkspaceFolder: () => ({ uri: { fsPath: path.resolve('.') } }),
      onDidCloseTextDocument: event('close'), onDidChangeConfiguration: event('config'), onDidGrantWorkspaceTrust: event('trust') },
  };
  const controller = registerSymbols(vscode, { appendLine: line => lines.push(line) }, {
    launch: async input => { launches.push(input); return options.launch ? options.launch(input) : {}; },
    start: (_launch, source, config) => {
      const result = deferred(), cleanup = deferred();
      const job = { source, config, completion: result.promise, cleanup: cleanup.promise, stopped: 0,
        finish(value) { result.resolve(value); cleanup.resolve(); }, fail(error) { result.reject(error); cleanup.resolve(); },
        finishResult: result.resolve, finishCleanup: cleanup.resolve,
        stop() { this.stopped++; if (options.stopThrows) throw new Error('cannot terminate'); } };
      requests.push(job); return job;
    },
  });
  return { controller, doc, token, requests, launches, settings, vscode, events, lines, registrations };
}
const result = { diagnostics: [], symbols: [{ name: 'item', kind: 'variable', start: 0, end: 10, selectionStart: 4, selectionEnd: 8 }] };

test('Outline forwards exact buffers, uses parser kinds/ranges, and caches by version and configuration', async () => {
  const h = host();
  const pending = h.controller.provideDocumentSymbols(h.doc, h.token()); await tick();
  assert.equal(h.launches[0].symbols, true); assert.equal(h.launches[0].check, true);
  assert.equal(h.requests[0].source, h.doc.text);
  h.requests[0].finish(result);
  const symbols = await pending;
  assert.equal(symbols[0].name, 'item'); assert.equal(symbols[0].kind, 1);
  assert.equal(symbols[0].selectionRange.start, 4);
  assert.equal(await h.controller.provideDocumentSymbols(h.doc, h.token()), symbols);
  h.settings.executablePath = 'another-solx';
  const changed = h.controller.provideDocumentSymbols(h.doc, h.token()); await tick();
  assert.equal(h.requests.length, 2); h.requests[1].finish(result); await changed;
  await h.controller.dispose();
});

test('Outline skips disabled, untrusted, closed, unsupported, oversized and cancelled buffers', async () => {
  for (const mode of ['disabled', 'untrusted', 'closed', 'scheme', 'large', 'cancelled']) {
    const h = host(), token = h.token();
    if (mode === 'disabled') h.settings['symbols.enabled'] = false;
    if (mode === 'untrusted') h.vscode.workspace.isTrusted = false;
    if (mode === 'closed') h.doc.isClosed = true;
    if (mode === 'scheme') h.doc.uri.scheme = 'git';
    if (mode === 'large') h.doc.text = ' '.repeat(1_048_577);
    if (mode === 'cancelled') token.cancel();
    assert.deepEqual(await h.controller.provideDocumentSymbols(h.doc, token), []);
    assert.equal(h.launches.length, 0); await h.controller.dispose();
  }
});

test('configuration changes invalidate both the editor provider identity and unchanged-buffer symbol cache', async () => {
  const h = host();
  const first = h.registrations[0].provider.provideDocumentSymbols(h.doc, h.token()); await tick();
  h.requests[0].finish(result); assert.equal((await first)[0].name, 'item');
  const configure = (key, value) => {
    h.settings[key] = value;
    h.events.config({ affectsConfiguration: name => name === `solx.${key}` });
  };
  configure('hover.enabled', false);
  assert.equal(h.registrations.length, 1, 'Unrelated changes must not restart Outline');
  configure('symbols.enabled', false);
  assert.ok(h.registrations[0].disposed);
  assert.notEqual(h.registrations[1].provider, h.registrations[0].provider);
  assert.deepEqual(await h.registrations[1].provider.provideDocumentSymbols(h.doc, h.token()), []);
  assert.equal(h.requests.length, 1, 'Disabled Outline must not launch a checker');
  configure('symbols.enabled', true);
  assert.ok(h.registrations[1].disposed);
  const restored = h.registrations[2].provider.provideDocumentSymbols(h.doc, h.token()); await tick();
  assert.equal(h.doc.version, 1);
  assert.equal(h.requests.length, 2, 'Enabling must refresh the unchanged buffer');
  h.requests[1].finish(result); assert.equal((await restored)[0].name, 'item');
  configure('cargoManifestPath', 'new/Cargo.toml');
  assert.ok(h.registrations[2].disposed);
  const reconfigured = h.registrations[3].provider.provideDocumentSymbols(h.doc, h.token()); await tick();
  assert.equal(h.launches[2].cargoManifestPath, 'new/Cargo.toml');
  h.requests[2].finish(result); await reconfigured;
  await h.controller.dispose();
  assert.ok(h.registrations.every(registration => registration.disposed));
});

test('concurrent symbol requests share one checker and both receive complete results', async () => {
  const h = host();
  const first = h.controller.provideDocumentSymbols(h.doc, h.token());
  const second = h.controller.provideDocumentSymbols(h.doc, h.token()); await tick();
  assert.equal(h.requests.length, 1);
  assert.equal(h.requests[0].stopped, 0);
  h.requests[0].finish(result);
  assert.equal((await first)[0].name, 'item');
  assert.equal((await second)[0].name, 'item');
  await h.controller.dispose();
});

test('one symbol consumer can cancel without cancelling another, while cancelling all stops the checker', async () => {
  for (const cancelAll of [false, true]) {
    const h = host(), token1 = h.token(), token2 = h.token();
    const first = h.controller.provideDocumentSymbols(h.doc, token1);
    const second = h.controller.provideDocumentSymbols(h.doc, token2); await tick();
    token1.cancel();
    assert.deepEqual(await first, []);
    assert.equal(h.requests[0].stopped, 0);
    if (cancelAll) {
      token2.cancel(); assert.equal(h.requests[0].stopped, 1);
      assert.deepEqual(await second, []);
    }
    h.requests[0].finish(result);
    if (!cancelAll) assert.equal((await second)[0].name, 'item');
    await h.controller.dispose();
  }
});

test('granting workspace trust refreshes the editor provider without editing the document', async () => {
  const h = host({ trusted: false });
  assert.deepEqual(await h.controller.provideDocumentSymbols(h.doc, h.token()), []);
  h.vscode.workspace.isTrusted = true; h.events.trust();
  assert.ok(h.registrations[0].disposed);
  const trusted = h.registrations[1].provider.provideDocumentSymbols(h.doc, h.token()); await tick();
  assert.equal(h.doc.version, 1);
  h.requests[0].finish(result); assert.equal((await trusted)[0].name, 'item');
  await h.controller.dispose();
});

test('obsolete symbol results cannot replace newer versions and shutdown awaits superseded cleanup', async () => {
  const h = host();
  const first = h.controller.provideDocumentSymbols(h.doc, h.token()); await tick();
  h.doc.version++;
  const second = h.controller.provideDocumentSymbols(h.doc, h.token()); await tick();
  assert.equal(h.requests[0].stopped, 1);
  h.requests[0].finishResult(result);
  h.requests[1].finish(result); await second;
  let finished = false;
  const cleanup = h.controller.dispose().then(() => { finished = true; });
  await tick(); assert.equal(finished, false);
  h.requests[0].finishCleanup();
  assert.deepEqual(await first, []); await cleanup; assert.equal(finished, true);
});

test('cancellation during preparation, close, configuration, process failure and stop failure are safe', async () => {
  const preparation = deferred(), h = host({ launch: () => preparation.promise });
  const token = h.token(), pending = h.controller.provideDocumentSymbols(h.doc, token);
  token.cancel(); preparation.resolve({});
  assert.deepEqual(await pending, []); assert.equal(h.requests.length, 0); await h.controller.dispose();
  for (const mode of ['close', 'config', 'failure', 'stop']) {
    const current = host({ stopThrows: mode === 'stop' }), token = current.token();
    const pending = current.controller.provideDocumentSymbols(current.doc, token); await tick();
    if (mode === 'close') current.events.close(current.doc);
    if (mode === 'config') current.events.config({ affectsConfiguration: () => true });
    if (mode === 'stop') token.cancel();
    if (mode === 'failure') current.requests[0].fail(new Error('unsupported symbols'));
    else current.requests[0].finish(result);
    assert.deepEqual(await pending, []);
    if (mode === 'failure' || mode === 'stop') assert.ok(current.lines.length);
    await current.controller.dispose();
  }
});
