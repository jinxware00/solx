const { test } = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const { cliArguments } = require('../scripts/vscode-cli.cjs');
const root = path.resolve('Code with spaces');
const executable = path.join(root, 'Code.exe');

test('packaged host tests use the actual CLI path for old and new Windows VS Code layouts', () => {
  const options = { platform: 'win32', exists: () => true };
  assert.deepEqual(cliArguments(executable, { ...options, read: () => '"%~dp0..\\Code.exe" "%~dp0..\\resources\\app\\out\\cli.js" --ms-enable-electron-run-as-node %*' }),
    [path.join(root, 'resources/app/out/cli.js'), '--ms-enable-electron-run-as-node']);
  assert.deepEqual(cliArguments(executable, { ...options, read: () => '"%~dp0..\\Code.exe" "%~dp0..\\abc123\\resources\\app\\out\\cli.js" %*' }),
    [path.join(root, 'abc123/resources/app/out/cli.js')]);
});

test('CLI discovery rejects missing scripts and paths outside the installation', () => {
  assert.throws(() => cliArguments(executable, { platform: 'win32', exists: () => false }), /not found/);
  assert.throws(() => cliArguments(executable, { platform: 'win32', exists: () => true, read: () => '"%~dp0..\\..\\resources\\app\\out\\cli.js"' }), /escapes/);
});
