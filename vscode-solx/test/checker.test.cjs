const { test } = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const { EventEmitter } = require('node:events');
const { PassThrough } = require('node:stream');
const { decodeReport, startCheck: check, MAX_SOURCE_BYTES } = require('../src/checker.cjs');
const startCheck = (launch, source, options) => check({ args: [], ...launch }, source, { platform: 'linux', ...options });
const { createLaunch } = require('../src/runner.cjs');
const report = (diagnostics = []) => ({ schemaVersion: 1, offsetEncoding: 'utf-8', source: '<stdin>', ok: !diagnostics.length, diagnostics, error: null });
const diagnostic = (start, end) => ({ severity: 'error', code: 'parse-error', message: 'expected expression', range: { start, end } });

test('JSON byte ranges map to UTF-16 without splitting emoji or losing CRLF offsets', () => {
  const source = "var text='😀é'\r\nvar bad=)";
  const byte = Buffer.byteLength(source.slice(0, source.indexOf(')')));
  const [item] = decodeReport(source, JSON.stringify(report([diagnostic(byte, byte + 1)])), 1);
  assert.equal(source.slice(item.start, item.end), ')');
  assert.equal(item.start, source.indexOf(')'));
  const eof = Buffer.byteLength(source);
  assert.equal(decodeReport(source, JSON.stringify(report([diagnostic(eof, eof)])), 1)[0].start, source.length);
  assert.deepEqual(decodeReport('', JSON.stringify(report()), 0), []);
});

test('malformed, incompatible, contradictory and out-of-range responses are rejected', () => {
  for (const [response, code] of [
    ['not JSON', 1], [{ ...report(), schemaVersion: 2 }, 0], [{ ...report(), offsetEncoding: 'utf-16' }, 0],
    [report(), 1], [report([diagnostic(0, 1)]), 0], [report([diagnostic(-1, 1)]), 1],
    [report([diagnostic(0, 999)]), 1], [report([diagnostic(1, 0)]), 1], [report([diagnostic(0.5, 1)]), 1],
    [report([diagnostic(1, 2)]), 1], // both offsets split the source's emoji
    [report([{ ...diagnostic(0, 4), message: null }]), 1],
    [{ ...report(), error: { code: 'input-error', message: 'too large' } }, 0],
  ]) assert.throws(() => decodeReport('😀', typeof response === 'string' ? response : JSON.stringify(response), code));
  assert.throws(() => decodeReport('', JSON.stringify({ ...report(), ok: false, error: { code: 'input-error', message: 'too large' } }), 2), /too large/);
});

test('check launch preserves executable discovery and Cargo freshness with no source in arguments', async () => {
  const file = path.resolve('path with spaces', "quote' & file.solx");
  const dependencies = { findExecutable: async name => name, realpath: async value => value, stat: async () => ({ isFile: () => true }) };
  const launch = await createLaunch({ file, check: true }, dependencies);
  assert.deepEqual(launch.args, ['check', '--json']);
  const cargo = await createLaunch({ file, check: true, workspace: path.dirname(file), cargoManifestPath: 'Cargo.toml' }, dependencies);
  assert.deepEqual(cargo.args.slice(-3), ['--', 'check', '--json']);
});

test('checker separates stderr from JSON, sends exact stdin, and reports EPIPE safely', async () => {
  let options, input = '';
  const source = "var text = '😀'\r\n";
  const child = new EventEmitter();
  child.pid = 123;
  child.stdout = new PassThrough(); child.stderr = new PassThrough(); child.stdin = new PassThrough();
  child.stdin.setEncoding('utf8'); child.stdin.on('data', data => { input += data; });
  const run = startCheck({}, source, { spawn: (_exe, _args, received) => { options = received; return child; } });
  child.stderr.write('Cargo progress\n'); child.stdout.write(JSON.stringify(report())); child.emit('close', 0, null);
  assert.deepEqual(await run.completion, { diagnostics: [] });
  assert.equal(input, source);
  assert.equal(options.shell, false); assert.equal(options.windowsHide, true); assert.equal(options.stdio[0], 'pipe');
  const broken = startCheck({}, source, { start: (_launch, _output, streams) => {
    streams.onStdout(JSON.stringify(report()));
    return { completion: Promise.resolve({ code: 0, inputError: new Error('EPIPE') }), stop() {} };
  } });
  await assert.rejects(broken.completion, /EPIPE/);
});

test('checker handles old CLIs, launch failures, output limits, timeouts and cancellation', async () => {
  for (const [result, stdout, expected] of [
    [{ code: 2 }, '', /supporting check/], [{ error: new Error('ENOENT') }, '', /ENOENT/],
    [{ truncated: true }, '', /output limit/], [{ code: 0 }, '{}', /protocol/],
  ]) {
    const run = startCheck({}, '', { start: (_launch, _output, streams) => {
      streams.onStdout(stdout); streams.onStderr('old CLI or process error');
      return { completion: Promise.resolve(result), stop() {} };
    } });
    await assert.rejects(run.completion, expected);
  }
  let finish, stopped = false;
  const timed = startCheck({}, '', { timeoutMs: 10, start: () => ({ completion: new Promise(resolve => { finish = resolve; }), stop() { stopped = true; finish({ cancelled: true }); } }) });
  await assert.rejects(timed.completion, /timed out/); assert.ok(stopped);
  const stuck = startCheck({}, '', { timeoutMs: 10, start: () => ({ completion: new Promise(() => {}), stop() { throw new Error('stop failed'); } }) });
  await assert.rejects(stuck.completion, /timed out/);
  const cancelled = startCheck({}, '', { start: () => ({ completion: Promise.resolve({ cancelled: true }), stop() {} }) });
  assert.deepEqual(await cancelled.completion, { cancelled: true });
  assert.throws(() => startCheck({}, ' '.repeat(MAX_SOURCE_BYTES + 1)), /1 MiB/);
});

test('Windows checks use a snapshot path as a separate argument, including Cargo launches', async () => {
  for (const args of [['check', '--json'], ['run', '--manifest-path', 'Cargo.toml', '--', 'check', '--json']]) {
    const file = path.resolve("space & quote' folder", 'buffer.solx');
    const source = "var text='😀é'\r\n";
    let disposed = 0, launched;
    const run = startCheck({ args }, source, { platform: 'win32',
      snapshot: async text => { assert.equal(text, source); return { file, async dispose() { disposed++; } }; },
      start: (command, _output, options) => {
        launched = command;
        assert.equal(options.input, undefined);
        options.onStdout(JSON.stringify({ ...report(), source: file }));
        return { completion: Promise.resolve({ code: 0 }), stop() {} };
      },
    });
    assert.deepEqual(await run.completion, { diagnostics: [] });
    assert.deepEqual(launched.args, [...args, '--', file]);
    assert.ok(!launched.args.includes(source));
    assert.equal(disposed, 1);
  }
});

test('cancellation and timeout during snapshot preparation prevent launching and clean up late files', async () => {
  for (const timeout of [false, true]) {
    let ready, disposed = 0, launches = 0;
    const run = startCheck({}, 'var x=1', { platform: 'win32', timeoutMs: 10,
      snapshot: () => new Promise(resolve => { ready = resolve; }),
      start() { launches++; throw new Error('must not launch'); },
    });
    if (timeout) await assert.rejects(run.completion, /timed out/);
    else run.stop();
    ready({ file: 'snapshot.solx', async dispose() { disposed++; } });
    if (!timeout) assert.deepEqual(await run.completion, { cancelled: true });
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(disposed, 1); assert.equal(launches, 0);
  }
});

test('Windows snapshots are removed after launch, process, protocol and output failures', async () => {
  for (const failure of ['throw', 'launch', 'protocol', 'output']) {
    let disposed = 0;
    const run = startCheck({}, 'var x=1', { platform: 'win32',
      snapshot: async () => ({ file: 'snapshot.solx', async dispose() { disposed++; } }),
      start() {
        if (failure === 'throw') throw new Error('spawn failed');
        const result = failure === 'launch' ? { error: new Error('ENOENT') }
          : failure === 'output' ? { truncated: true } : { code: 2 };
        return { completion: Promise.resolve(result), stop() {} };
      },
    });
    await assert.rejects(run.completion);
    assert.equal(disposed, 1);
  }
});

test('Windows cancellation and process timeouts remove snapshots even when termination throws', async () => {
  for (const timeout of [false, true]) {
    let disposed = 0, stopped = 0, finish;
    const run = startCheck({}, 'var x=1', { platform: 'win32', timeoutMs: 10,
      snapshot: async () => ({ file: 'snapshot.solx', async dispose() { disposed++; } }),
      start: () => ({ completion: new Promise(resolve => { finish = resolve; }), stop() {
        stopped++;
        if (timeout) throw new Error('stop failed');
        finish({ cancelled: true });
      } }),
    });
    await new Promise(resolve => setImmediate(resolve));
    if (timeout) await assert.rejects(run.completion, /timed out/);
    else { run.stop(); assert.deepEqual(await run.completion, { cancelled: true }); }
    assert.equal(stopped, 1); assert.equal(disposed, 1);
  }
});

test('snapshot preparation and cleanup failures are operational errors, not clean syntax results', async () => {
  for (const failure of ['prepare', 'cleanup']) {
    const run = startCheck({}, '', { platform: 'win32', snapshot: async () => {
      if (failure === 'prepare') throw new Error('cannot create snapshot');
      return { file: 'snapshot.solx', async dispose() { throw new Error('cannot remove snapshot'); } };
    }, start: (_command, _output, options) => {
      options.onStdout(JSON.stringify(report()));
      return { completion: Promise.resolve({ code: 0 }), stop() {} };
    } });
    await assert.rejects(run.completion, /cannot (create|remove) snapshot/);
  }
});
