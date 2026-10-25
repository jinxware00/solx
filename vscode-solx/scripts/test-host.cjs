const fs = require('node:fs');
const path = require('node:path');
const cp = require('node:child_process');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const { cliArguments } = require('./vscode-cli.cjs');
const root = path.resolve(__dirname, '..');
async function main() {
const timeoutMs = Number(process.env.SOLX_HOST_TIMEOUT_MS || 600_000);
if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1000 || timeoutMs > 600_000) {
  throw new Error('SOLX_HOST_TIMEOUT_MS must be an integer from 1000 to 600000.');
}
const candidates = [process.env.VSCODE_EXECUTABLE];
if (process.platform === 'win32') {
  for (const directory of [process.env.LOCALAPPDATA && path.join(process.env.LOCALAPPDATA, 'Programs'), process.env.ProgramFiles]) {
    if (directory) candidates.push(path.join(directory, 'Microsoft VS Code', 'Code.exe'));
  }
} else {
  candidates.push('/Applications/Visual Studio Code.app/Contents/MacOS/Electron', '/usr/share/code/code', '/usr/lib/code/code');
}
const packaged = process.argv.includes('--package');
const version = process.argv.slice(2).find(arg => /^\d+\.\d+\.\d+$/.test(arg));
const executable = version
  ? await require('@vscode/test-electron').downloadAndUnzipVSCode({ version, cachePath: path.join(root, '.test-artifacts/vscode') })
  : candidates.find(candidate => candidate && fs.existsSync(candidate));
if (!executable) throw new Error('Desktop VS Code not found. Set VSCODE_EXECUTABLE to its executable.');
fs.mkdirSync(path.join(root, '.test-artifacts'), { recursive: true });
const artifacts = fs.mkdtempSync(path.join(root, '.test-artifacts/host-'));
const workspace = path.join(artifacts, 'workspace');
fs.mkdirSync(workspace);
const temporary = path.join(artifacts, 'temp');
fs.mkdirSync(temporary);
const resultFile = path.join(artifacts, 'results.json');
const extensionDir = path.join(artifacts, 'extensions');
const userData = path.join(artifacts, 'user-data');
// Built-in chat/Git services can start even with --disable-extensions. Keep
// unrelated background work out of this fresh test profile and its job budget.
// No Solx setting or normal editor profile is changed.
fs.mkdirSync(path.join(userData, 'User'), { recursive: true });
fs.writeFileSync(path.join(userData, 'User', 'settings.json'), JSON.stringify({
  'chat.disableAIFeatures': true,
  'git.enabled': false,
  'git.autofetch': false,
  'telemetry.telemetryLevel': 'off',
  'update.mode': 'none',
}, null, 2) + '\n');
let extensionPath = root;
if (packaged) {
  const pkg = require('../package.json');
  const vsix = path.join(root, `${pkg.name}-${pkg.version}.vsix`);
  if (!fs.existsSync(vsix)) throw new Error('Build the current VSIX with npm run package first.');
  // This is the same entry point used by the official code.cmd wrapper.
  const install = cp.spawnSync(executable, [...cliArguments(executable), '--user-data-dir', userData,
    '--extensions-dir', extensionDir, '--install-extension', vsix, '--force'], {
    env: { ...process.env, VSCODE_DEV: '', ELECTRON_RUN_AS_NODE: '1' }, encoding: 'utf8', windowsHide: true, timeout: 60_000,
  });
  fs.writeFileSync(path.join(artifacts, 'install.log'), (install.stdout || '') + (install.stderr || ''));
  assert.equal(install.status, 0, install.error?.message || install.stderr || install.stdout);
  extensionPath = path.join(extensionDir, `${pkg.publisher}.${pkg.name}-${pkg.version}`);
  const installed = JSON.parse(fs.readFileSync(path.join(extensionPath, 'package.json'), 'utf8'));
  // VS Code adds installation metadata, but every shipped manifest field
  // (including settings and engine compatibility) must match this checkout.
  const { __metadata, ...installedManifest } = installed;
  assert.deepEqual(installedManifest, pkg, 'Packaged manifest does not match the current source');
  const files = fs.readdirSync(extensionPath, { recursive: true }).filter(file => fs.statSync(path.join(extensionPath, file)).isFile());
  const required = ['README.md', 'LICENSE', 'CHANGELOG.md', 'images/icon.png', 'syntaxes/solx.tmLanguage.json',
    'language-configuration.json', 'data/snippets.json', 'examples/hello.solx',
    ...fs.readdirSync(path.join(root, 'src')).map(file => `src/${file}`)];
  const hash = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
  const packagedName = file => file === 'LICENSE' ? 'LICENSE.txt' : file === 'README.md' || file === 'CHANGELOG.md' ? file.toLowerCase() : file;
  for (const file of required) assert.equal(hash(path.join(extensionPath, packagedName(file))), hash(path.join(root, file)), file);
  for (const file of files) assert.ok([...required.map(packagedName), 'package.json', '.vsixmanifest'].includes(file.replaceAll('\\', '/')), `Unexpected packaged file: ${file}`);
  fs.writeFileSync(path.join(artifacts, 'package.json'), JSON.stringify({ vsix, sha256: hash(vsix), files }, null, 2));
  console.log(`Installed and verified ${pkg.publisher}.${pkg.name}@${pkg.version} from VSIX (${files.length} files).`);
}
const env = { ...process.env, SOLX_HOST_RESULTS: resultFile, SOLX_EXTENSION_PATH: extensionPath,
  CARGO_TARGET_DIR: path.join(root, '.test-artifacts/cargo') };
// Isolate all temporary snapshots so leaked source fails this run, without
// inspecting or deleting temporary files owned by another editor/profile.
if (process.platform === 'win32') { env.TEMP = temporary; env.TMP = temporary; }
else env.TMPDIR = temporary;
delete env.ELECTRON_RUN_AS_NODE;
const child = cp.spawn(executable, [
  '--user-data-dir', userData,
  '--extensions-dir', extensionDir,
  '--extensionDevelopmentPath=' + extensionPath,
  '--extensionTestsPath=' + path.join(root, 'test/host/index.cjs'),
  '--disable-extensions', '--disable-workspace-trust', '--skip-welcome', '--skip-release-notes',
  workspace,
], { env, windowsHide: true, detached: process.platform !== 'win32', stdio: ['ignore', 'pipe', 'pipe'] });
// Keep draining noisy hosts without allowing diagnostic output to grow the
// harness heap indefinitely. This is not a memory limit for the editor tree.
let log = '', truncated = false, timedOut = false;
const capture = data => {
  log += data;
  if (log.length > 2_000_000) { log = log.slice(-2_000_000); truncated = true; }
};
child.stdout.on('data', capture);
child.stderr.on('data', capture);
child.on('error', error => { console.error(error); process.exitCode = 1; });
const timeout = setTimeout(() => {
  timedOut = true;
  console.error(`Extension Host timed out after ${timeoutMs} ms.`);
  if (!child.pid) return;
  if (process.platform === 'win32') {
    const killed = cp.spawnSync(path.join(process.env.SystemRoot || 'C:\\Windows', 'System32/taskkill.exe'),
      ['/PID', String(child.pid), '/T', '/F'], { windowsHide: true, timeout: 10_000 });
    if (killed.status !== 0) child.kill();
  } else {
    try { process.kill(-child.pid, 'SIGKILL'); }
    catch { child.kill('SIGKILL'); }
  }
}, timeoutMs);
child.on('close', code => {
  clearTimeout(timeout);
  fs.writeFileSync(path.join(artifacts, 'host.log'), (truncated ? '[Earlier host output discarded after log limit]\n' : '') + log);
  const report = fs.existsSync(resultFile) ? JSON.parse(fs.readFileSync(resultFile, 'utf8')) : undefined;
  const snapshots = fs.readdirSync(temporary).filter(file => file.startsWith('solx-check-'));
  if (snapshots.length) console.error('Snapshot cleanup failed:', snapshots);
  if (timedOut || code !== 0 || !report?.success || snapshots.length) {
    console.error(log.slice(-8000));
    console.error(report || 'No Extension Host results were produced.');
    process.exitCode = 1;
  } else {
    console.log(`VS Code ${report.vscode}: ${report.checks.length} Extension Host checks passed; no temporary snapshots remain.`);
    if (packaged) {
      const verified = JSON.parse(fs.readFileSync(path.join(artifacts, 'package.json'), 'utf8'));
      fs.writeFileSync(path.join(root, '.test-artifacts/package-validation.json'), JSON.stringify({
        sha256: verified.sha256, vscode: report.vscode, checks: report.checks.length, success: true,
        snapshotsRemaining: snapshots.length,
      }, null, 2));
    }
  }
  console.log(`Host results: ${artifacts}`);
});
}
main().catch(error => { console.error(error); process.exitCode = 1; });
