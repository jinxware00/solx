const fs = require('node:fs/promises');
const constants = require('node:fs').constants;
const path = require('node:path');
const cp = require('node:child_process');

function resolveSetting(value, workspace, paths = path) {
  if (paths.isAbsolute(value)) return value;
  if (!workspace) throw new Error('Relative paths require an open workspace folder. Use an absolute path instead.');
  return paths.resolve(workspace, value);
}

async function findExecutable(value, { workspace, platform = process.platform, env = process.env, access = fs.access } = {}) {
  const paths = platform === 'win32' ? path.win32 : path.posix;
  if (!value || /[\r\n\0]/.test(value)) throw new Error('Set an executable path or command name without arguments.');
  if (/^['"]|['"]$/.test(value)) throw new Error('Do not add quotes or arguments to the executable path. Spaces are supported directly.');
  const explicit = paths.isAbsolute(value) || /[\\/]/.test(value);
  const pathKey = Object.keys(env).find(key => key.toLowerCase() === 'path');
  const directories = (env[pathKey] || '').split(paths.delimiter)
    .map(dir => dir.replace(/^"|"$/g, ''))
    .filter(dir => paths.isAbsolute(dir));
  const candidates = explicit ? [resolveSetting(value, workspace, paths)] : directories.map(dir => paths.join(dir, value));
  for (let candidate of candidates) {
    if (platform === 'win32') {
      if (!paths.extname(candidate)) candidate += '.exe';
      if (paths.extname(candidate).toLowerCase() !== '.exe') continue;
    }
    try {
      await access(candidate, platform === 'win32' ? constants.F_OK : constants.X_OK);
      return candidate;
    } catch { /* Try the next absolute PATH entry. Never search the current directory implicitly. */ }
  }
  throw new Error(`Executable not found: ${value}. Configure solx.executablePath or install Solx on PATH. On Windows use an .exe, not a shell script.`);
}

function isCargoArtifact(executable) {
  return /(?:^|[\\/])target[\\/].*[\\/](?:solx|solx\.exe)$/i.test(executable);
}

async function createLaunch({ file, executablePath = '', cargoManifestPath = '', workspace, check = false }, dependencies = {}) {
  const find = dependencies.findExecutable || findExecutable;
  const stat = dependencies.stat || fs.stat;
  const realpath = dependencies.realpath || fs.realpath;
  if (!path.isAbsolute(file)) throw new Error('Run requires an absolute, saved file path.');
  if (executablePath && cargoManifestPath) throw new Error('Configure either solx.executablePath or solx.cargoManifestPath, not both.');
  const cwd = path.dirname(file);
  if (cargoManifestPath) {
    const manifest = resolveSetting(cargoManifestPath, workspace);
    if (path.basename(manifest) !== 'Cargo.toml' || !(await stat(manifest)).isFile()) {
      throw new Error('solx.cargoManifestPath must point to the Solx Cargo.toml file.');
    }
    const executable = await find('cargo', { workspace });
    return { executable, args: ['run', '--quiet', '--locked', '--manifest-path', manifest, '--bin', 'solx', '--', ...(check ? ['check', '--json', '--stdin'] : [file])], cwd };
  }
  const executable = await find(executablePath || 'solx', { workspace });
  // Refuse build-tree binaries, including symlinks to them. Cargo mode checks
  // source freshness on every run; an installed interpreter is user-managed.
  if (isCargoArtifact(executable) || isCargoArtifact(await realpath(executable))) {
    throw new Error('This executable is in a Cargo target directory and may be stale. Set solx.cargoManifestPath to the Solx Cargo.toml and clear solx.executablePath to rebuild before running.');
  }
  return { executable, args: check ? ['check', '--json', '--stdin'] : ['--', file], cwd };
}

function startProcess(launch, onOutput, { spawn = cp.spawn, platform = process.platform, env = process.env, kill = process.kill, maxOutput = 2_000_000, input, onStdout = onOutput, onStderr = onOutput } = {}) {
  let child, settled = false, cancelled = false, outputSize = 0, truncated = false, inputError;
  let finish;
  const completion = new Promise(resolve => { finish = result => {
    if (!settled) { settled = true; resolve({ ...result, cancelled, truncated, inputError }); }
  }; });
  const output = (chunk, write) => {
    if (outputSize >= maxOutput) return;
    const remaining = maxOutput - outputSize;
    write(chunk.slice(0, remaining));
    outputSize += chunk.length;
    if (outputSize >= maxOutput && !truncated) {
      truncated = true;
      onOutput('\n[Output limit reached; remaining output is discarded. Use Solx: Stop Running File if needed.]\n');
    }
  };
  try {
    child = spawn(launch.executable, launch.args, {
      cwd: launch.cwd, shell: false, windowsHide: true,
      detached: platform !== 'win32', stdio: [input === undefined ? 'ignore' : 'pipe', 'pipe', 'pipe'],
    });
    for (const [stream, write] of [[child.stdout, onStdout], [child.stderr, onStderr]]) {
      stream.setEncoding('utf8');
      stream.on('data', chunk => output(chunk, write));
    }
    child.once('error', error => finish({ error }));
    child.once('close', (code, signal) => finish({ code, signal }));
    if (input !== undefined) {
      // Older CLIs can close stdin before accepting the buffer. Drain/report
      // EPIPE rather than letting an unhandled stream error crash the host.
      child.stdin.on('error', error => { inputError = error; });
      child.stdin.end(input, 'utf8');
    }
  } catch (error) { finish({ error }); }
  const stop = () => {
    if (settled || cancelled || !child?.pid) return;
    cancelled = true;
    if (platform === 'win32') {
      // Kill only the process tree started by this run, including Cargo's child.
      const command = path.win32.join(env.SystemRoot || 'C:\\Windows', 'System32', 'taskkill.exe');
      const fallback = () => {
        if (settled) return;
        try {
          if (child.kill() === false) throw new Error('The process could not be terminated.');
        } catch (error) {
          cancelled = false;
          onOutput(`\n[Stop failed: ${error.message} Try Stop Running File again.]\n`);
        }
      };
      try {
        const killer = spawn(command, ['/PID', String(child.pid), '/T', '/F'], { shell: false, windowsHide: true, stdio: 'ignore' });
        killer.once('error', fallback);
        killer.once('close', code => { if (code !== 0) fallback(); });
      } catch { fallback(); }
    } else {
      try { kill(-child.pid, 'SIGKILL'); } catch {
        try {
          if (child.kill('SIGKILL') === false) throw new Error('The process could not be terminated.');
        } catch (error) {
          cancelled = false;
          onOutput(`\n[Stop failed: ${error.message} Try stopping again.]\n`);
        }
      }
    }
  };
  return { completion, stop };
}

module.exports = { findExecutable, createLaunch, startProcess, isCargoArtifact };
