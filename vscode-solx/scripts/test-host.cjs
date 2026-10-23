const fs = require('node:fs');
const path = require('node:path');
const cp = require('node:child_process');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const { cliArguments } = require('./vscode-cli.cjs');
const root = path.resolve(__dirname, '..');
async function main() {
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
let extensionPath = root;
if (packaged) {
  const pkg = require('../package.json');
  const vsix = path.join(root, `${pkg.name}-${pkg.version}.vsix`);
  if (!fs.existsSync(vsix)) throw new Error('Build the current VSIX with npm run package first.');
  // This is the same entry point used by the official code.cmd wrapper.
  const install = cp.spawnSync(executable, [...cliArguments(executable), '--user-data-dir', path.join(artifacts, 'user-data'),
    '--extensions-dir', extensionDir, '--install-extension', vsix, '--force'], {
    env: { ...process.env, VSCODE_DEV: '', ELECTRON_RUN_AS_NODE: '1' }, encoding: 'utf8', windowsHide: true, timeout: 60_000,
  });
  fs.writeFileSync(path.join(artifacts, 'install.log'), (install.stdout || '') + (install.stderr || ''));
  assert.equal(install.status, 0, install.error?.message || install.stderr || install.stdout);
  extensionPath = path.join(extensionDir, `${pkg.publisher}.${pkg.name}-${pkg.version}`);
  const installed = JSON.parse(fs.readFileSync(path.join(extensionPath, 'package.json'), 'utf8'));
  assert.equal(installed.publisher, pkg.publisher);
  assert.equal(installed.version, pkg.version);
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
  '--user-data-dir', path.join(artifacts, 'user-data'),
  '--extensions-dir', extensionDir,
  '--extensionDevelopmentPath=' + extensionPath,
  '--extensionTestsPath=' + path.join(root, 'test/host/index.cjs'),
  '--disable-extensions', '--disable-workspace-trust', '--skip-welcome', '--skip-release-notes',
  workspace,
], { env, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
let log = '';
child.stdout.on('data', data => { log += data; });
child.stderr.on('data', data => { log += data; });
child.on('error', error => { console.error(error); process.exitCode = 1; });
const timeout = setTimeout(() => {
  console.error('Extension Host timed out after 600 seconds.');
  if (process.platform === 'win32') {
    cp.spawnSync(path.join(process.env.SystemRoot, 'System32/taskkill.exe'), ['/PID', String(child.pid), '/T', '/F'], { windowsHide: true });
  } else child.kill('SIGKILL');
}, 600_000);
child.on('close', code => {
  clearTimeout(timeout);
  fs.writeFileSync(path.join(artifacts, 'host.log'), log);
  const report = fs.existsSync(resultFile) ? JSON.parse(fs.readFileSync(resultFile, 'utf8')) : undefined;
  const snapshots = fs.readdirSync(temporary).filter(file => file.startsWith('solx-check-'));
  if (snapshots.length) console.error('Snapshot cleanup failed:', snapshots);
  if (code !== 0 || !report?.success || snapshots.length) {
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
