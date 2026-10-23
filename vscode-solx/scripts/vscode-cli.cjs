const fs = require('node:fs');
const path = require('node:path');

function cliArguments(executable, { platform = process.platform, exists = fs.existsSync, read = fs.readFileSync } = {}) {
  if (platform === 'darwin') return [path.resolve(executable, '../../../Resources/app/out/cli.js')];
  const root = path.dirname(executable);
  let relative = 'resources/app/out/cli.js';
  let flag = platform === 'win32';
  const wrapper = path.join(root, 'bin/code.cmd');
  if (platform === 'win32' && exists(wrapper)) {
    const text = read(wrapper, 'utf8');
    const match = /"%~dp0\.\.\\((?:[A-Za-z0-9._-]+\\)*resources\\app\\out\\cli\.js)"/.exec(text);
    if (!match) throw new Error('Unrecognized VS Code CLI wrapper.');
    relative = match[1].split('\\').join(path.sep);
    flag = text.includes('--ms-enable-electron-run-as-node');
  }
  const cli = path.resolve(root, relative);
  if (path.relative(root, cli).startsWith('..' + path.sep)) throw new Error('VS Code CLI path escapes its installation.');
  if (!exists(cli)) throw new Error('VS Code CLI script not found.');
  return [cli, ...(flag ? ['--ms-enable-electron-run-as-node'] : [])];
}

module.exports = { cliArguments };
