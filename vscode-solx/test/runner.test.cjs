const { test } = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const { EventEmitter } = require('node:events');
const { PassThrough } = require('node:stream');
const { findExecutable, createLaunch, startProcess } = require('../src/runner.cjs');

test('Windows executable discovery preserves spaces and ignores implicit current directory', async () => {
  const visited = [];
  const executable = await findExecutable('solx', { platform: 'win32', env: { Path: ';relative;"C:\\Solx Tools";C:\\Other' }, access: async file => { visited.push(file); } });
  assert.equal(executable, 'C:\\Solx Tools\\solx.exe');
  assert.deepEqual(visited, [executable]);
  const explicit = await findExecutable('C:\\Solx Tools\\solx.exe', { platform: 'win32', access: async () => {} });
  assert.equal(explicit, executable);
  await assert.rejects(findExecutable('solx.cmd', { platform: 'win32', env: { PATH: 'C:\\Tools' }, access: async () => {} }), /Executable not found/);
  await assert.rejects(findExecutable('"C:\\Solx Tools\\solx.exe"'), /Do not add quotes/);
});

test('discovery reports missing executables and supports POSIX PATH', async () => {
  await assert.rejects(findExecutable('missing', { platform: 'linux', env: { PATH: '/tools' }, access: async () => { throw new Error('missing'); } }), /Executable not found/);
  assert.equal(await findExecutable('solx', { platform: 'linux', env: { PATH: '/tools' }, access: async () => {} }), '/tools/solx');
  await assert.rejects(findExecutable('./solx', { platform: 'linux' }), /open workspace/);
});

test('file arguments are passed separately, with option termination and the file directory as cwd', async () => {
  const file = path.resolve('path with spaces', '-test & quote\'.solx');
  const executable = path.resolve('installed tools', 'solx.exe');
  const launch = await createLaunch({ file }, { findExecutable: async () => executable, realpath: async p => p });
  assert.deepEqual(launch, { executable, args: ['--', file], cwd: path.dirname(file) });
});

test('Cargo mode rebuilds and never launches an existing target binary', async () => {
  const workspace = path.resolve('source checkout');
  const file = path.join(workspace, 'example.solx');
  const launch = await createLaunch({ file, workspace, cargoManifestPath: 'Cargo.toml' }, { findExecutable: async name => name, stat: async () => ({ isFile: () => true }) });
  assert.deepEqual(launch.args, ['run', '--quiet', '--locked', '--manifest-path', path.join(workspace, 'Cargo.toml'), '--bin', 'solx', '--', file]);
  for (const executable of [path.join(workspace, 'target/debug/solx.exe'), path.join(workspace, 'target/x86_64/release/solx')]) {
    await assert.rejects(createLaunch({ file }, { findExecutable: async () => executable, realpath: async p => p }), /may be stale/);
  }
  await assert.rejects(createLaunch({ file }, { findExecutable: async () => path.join(workspace, 'bin/solx'), realpath: async () => path.join(workspace, 'target/debug/solx') }), /may be stale/);
  await assert.rejects(createLaunch({ file, executablePath: 'solx', cargoManifestPath: 'Cargo.toml' }), /not both/);
  await assert.rejects(createLaunch({ file: 'relative.solx' }), /absolute/);
});

function fakeProcess() {
  const child = new EventEmitter();
  child.stdout = new PassThrough(); child.stderr = new PassThrough();
  child.pid = 123; child.kill = () => child.emit('close', null, 'SIGKILL');
  return child;
}

test('runner uses no shell, streams both outputs, and reports nonzero exit', async () => {
  const child = fakeProcess();
  let received, output = '';
  const launch = { executable: 'solx.exe', args: ['--', 'C:\\space & name.solx'], cwd: 'C:\\' };
  const run = startProcess(launch, text => { output += text; }, { platform: 'win32', spawn: (...args) => { received = args; return child; } });
  assert.equal(received[0], launch.executable);
  assert.deepEqual(received[1], launch.args);
  assert.equal(received[2].shell, false);
  assert.equal(received[2].windowsHide, true);
  child.stdout.write('hello\n'); child.stderr.write('error\n'); child.emit('close', 1, null);
  assert.equal((await run.completion).code, 1);
  assert.equal(output, 'hello\nerror\n');
});

test('runner handles asynchronous and synchronous launch failures', async () => {
  const child = fakeProcess();
  const run = startProcess({}, () => {}, { spawn: () => child });
  child.emit('error', new Error('ENOENT')); child.emit('close', -2);
  assert.equal((await run.completion).error.message, 'ENOENT');
  const failed = startProcess({}, () => {}, { spawn: () => { throw new Error('EACCES'); } });
  assert.equal((await failed.completion).error.message, 'EACCES');
});

test('stop terminates the owned process tree and output is bounded', async () => {
  const child = fakeProcess();
  const killer = new EventEmitter();
  let calls = [], output = '';
  const run = startProcess({}, text => { output += text; }, { platform: 'win32', env: { SystemRoot: 'C:\\Windows' }, maxOutput: 5, spawn: (...args) => { calls.push(args); return calls.length === 1 ? child : killer; } });
  child.stdout.write('123456789'); child.stdout.write('extra-output-sentinel');
  assert.match(output, /^12345\n\[Output limit/);
  assert.ok(!output.includes('extra-output-sentinel'));
  run.stop(); run.stop();
  assert.equal(calls.length, 2);
  assert.deepEqual(calls[1][1], ['/PID', '123', '/T', '/F']);
  child.emit('close', 1, null);
  assert.equal((await run.completion).cancelled, true);
});

test('a synchronous taskkill launch failure falls back to the owned child', async () => {
  const child = fakeProcess();
  let calls = 0;
  const run = startProcess({}, () => {}, { platform: 'win32', spawn: () => {
    if (calls++) throw new Error('taskkill unavailable');
    return child;
  } });
  assert.doesNotThrow(() => run.stop());
  assert.equal((await run.completion).cancelled, true);
});
