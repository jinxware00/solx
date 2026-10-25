#!/usr/bin/env node
'use strict';

// No package installation, shell execution, or automatic interpreter discovery.
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');

const root = path.resolve(__dirname, '..');
const benchmarkRoot = path.join(root, 'benchmarks');
const engines = ['solx', 'lua', 'luajit', 'luau'];
const help = `Usage: node scripts/benchmark.cjs --solx PATH [options]

  --lua PATH             Installed Lua executable (optional)
  --luajit PATH          Installed LuaJIT executable (optional)
  --luau PATH            Installed Luau executable (optional)
  --samples N            Measured launches per case/engine, 1..100 (default 7)
  --warmups N            Discarded launches per case/engine, 0..10 (default 2)
  --timeout-ms N         Hard timeout per launch, 100..60000 (default 10000)
  --out DIRECTORY        New results directory (default benchmarks/results/<time>)
  --build-profile LABEL  Solx build label: release, debug, custom, unknown
  --build-command TEXT   Record the Solx build command; never execute it
  --help                Show this help

Executable paths are explicit and resolved against the current directory.
Lua fixtures are used for LuaJIT; Luau has a separate module-loading fixture.
Lua initialization/search environment overrides are cleared. No binaries are downloaded.
All times are process wall time; startup is a separate baseline, not subtracted.
`;

function options(argv) {
  const result = { samples: 7, warmups: 2, timeoutMs: 10000, buildProfile: 'unknown' };
  const strings = new Map([
    ...engines.map(name => [`--${name}`, name]),
    ['--out', 'out'], ['--build-profile', 'buildProfile'], ['--build-command', 'buildCommand'],
  ]);
  const integers = new Map([
    ['--samples', ['samples', 1, 100]],
    ['--warmups', ['warmups', 0, 10]],
    ['--timeout-ms', ['timeoutMs', 100, 60000]],
  ]);
  const seen = new Set();
  for (let index = 0; index < argv.length; index += 1) {
    const key = argv[index];
    if (key === '--help') return { help: true };
    if ((!strings.has(key) && !integers.has(key)) || seen.has(key)) {
      throw new Error(`Unknown or repeated option: ${key}`);
    }
    seen.add(key);
    const value = argv[++index];
    if (value === undefined || value.length === 0) throw new Error(`Missing value for ${key}`);
    if (strings.has(key)) {
      result[strings.get(key)] = value;
    } else {
      const [name, minimum, maximum] = integers.get(key);
      const number = Number(value);
      if (!/^\d+$/.test(value) || !Number.isSafeInteger(number) || number < minimum || number > maximum) {
        throw new Error(`${key} must be an integer from ${minimum} to ${maximum}`);
      }
      result[name] = number;
    }
  }
  if (!result.solx) throw new Error('--solx PATH is required');
  if (!['release', 'debug', 'custom', 'unknown'].includes(result.buildProfile)) {
    throw new Error('--build-profile must be release, debug, custom, or unknown');
  }
  return result;
}

function digest(file) {
  return crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
}

function execute(executable, args, timeoutMs, env = process.env) {
  const start = process.hrtime.bigint();
  const output = spawnSync(executable, args, {
    cwd: root, env, encoding: 'utf8', windowsHide: true, shell: false,
    stdio: ['ignore', 'pipe', 'pipe'], timeout: timeoutMs,
    killSignal: 'SIGKILL', maxBuffer: 64 * 1024,
  });
  const elapsedNs = process.hrtime.bigint() - start;
  return {
    executable, args, elapsedNs: elapsedNs.toString(), elapsedMs: Number(elapsedNs) / 1e6,
    exitCode: output.status, signal: output.signal,
    error: output.error ? { code: output.error.code ?? null, message: output.error.message } : null,
    stdoutTruncated: (output.stdout ?? '').length > 4096,
    stderrTruncated: (output.stderr ?? '').length > 4096,
    stdout: (output.stdout ?? '').slice(0, 4096), stderr: (output.stderr ?? '').slice(0, 4096),
  };
}

function repositorySnapshot() {
  const files = ['Cargo.toml', 'Cargo.lock'];
  function visit(directory) {
    for (const entry of fs.readdirSync(path.join(root, directory), { withFileTypes: true })) {
      const relative = path.join(directory, entry.name);
      if (entry.isDirectory()) visit(relative);
      else if (entry.isFile() && entry.name.endsWith('.rs')) files.push(relative);
    }
  }
  visit('src');
  const revision = execute('git', ['rev-parse', 'HEAD'], 5000);
  const status = execute('git', ['status', '--porcelain=v1', '--untracked-files=normal'], 5000);
  return {
    commit: revision.exitCode === 0 ? revision.stdout.trim() : null,
    gitStatus: status.exitCode === 0 ? status.stdout : null,
    sourceSha256: Object.fromEntries(files.sort().map(file => [file.replaceAll('\\', '/'), digest(path.join(root, file))])),
  };
}

function summary(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return {
    count: sorted.length,
    medianMs: sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2,
    p95Ms: sorted[Math.ceil(sorted.length * 0.95) - 1],
    minMs: sorted[0], maxMs: sorted.at(-1),
  };
}

function fixture(relative) {
  const file = fs.realpathSync(path.resolve(benchmarkRoot, relative));
  const within = path.relative(benchmarkRoot, file);
  if (within.startsWith('..') || path.isAbsolute(within) || !fs.statSync(file).isFile()) {
    throw new Error(`Fixture escapes benchmarks directory: ${relative}`);
  }
  return file;
}

function main(config) {
  const manifestFile = path.join(benchmarkRoot, 'manifest.json');
  const manifest = JSON.parse(fs.readFileSync(manifestFile, 'utf8'));
  if (manifest.schemaVersion !== 1 || !Array.isArray(manifest.cases) || manifest.cases.length === 0) {
    throw new Error('Unsupported or empty benchmark manifest');
  }
  // Lua may execute initialization code or redirect require() through these
  // variables, even when the binary and intended fixture hashes are unchanged.
  // Preserve unrelated environment settings without recording private values.
  const runtimeEnvironment = { ...process.env };
  const removedEnvironmentNames = Object.keys(runtimeEnvironment)
    .filter(name => /^LUA_(?:INIT|PATH|CPATH)(?:_|$)/i.test(name)).sort();
  for (const name of removedEnvironmentNames) delete runtimeEnvironment[name];
  const selected = engines.filter(name => config[name]).map(name => {
    const executable = fs.realpathSync(path.resolve(config[name]));
    if (!fs.statSync(executable).isFile()) throw new Error(`Not an executable file: ${executable}`);
    const args = name === 'lua' || name === 'luajit' ? ['-v'] : ['--version'];
    const versionProbe = execute(executable, args, config.timeoutMs, runtimeEnvironment);
    return {
      name, executable, executableSha256: digest(executable), versionProbe,
      reportedVersion: !versionProbe.error && versionProbe.exitCode === 0
        ? (versionProbe.stdout + versionProbe.stderr).trim() : null,
    };
  });
  const cases = manifest.cases.map(entry => {
    if (!/^[a-z][a-z0-9_-]*$/.test(entry.name) || typeof entry.expectedStdout !== 'string') {
      throw new Error('Invalid benchmark manifest entry');
    }
    return {
      ...entry,
      supportFiles: (entry.support ?? []).map(relative => {
        const file = fixture(relative);
        return { path: file, sha256: digest(file) };
      }),
      files: Object.fromEntries(['solx', 'lua', ...(entry.luau ? ['luau'] : [])].map(name => {
        const file = fixture(entry[name]);
        return [name, { path: file, sha256: digest(file) }];
      })),
    };
  });
  const startedAt = new Date().toISOString();
  const directory = path.resolve(config.out ?? path.join(benchmarkRoot, 'results', `${startedAt.replaceAll(/[:.]/g, '-')}-${process.pid}`));
  fs.mkdirSync(path.dirname(directory), { recursive: true });
  fs.mkdirSync(directory); // Refuse to overwrite previous measurements.
  const report = {
    schemaVersion: 1, status: 'running', startedAt, finishedAt: null,
    invocation: { executable: process.execPath, args: process.argv.slice(1), cwd: process.cwd() },
    methodology: {
      clock: 'process.hrtime.bigint monotonic wall time around spawnSync',
      includes: ['process launch', 'runtime initialization', 'source read/parse or compile', 'program execution', 'checksum output', 'shutdown'],
      internalWorkTiming: null,
      limitation: 'No common reliable internal clock API is available. Startup baseline is separate and is never subtracted.',
      scheduling: 'Sequential cases; engine order rotates each warmup/sample; warmups launch fresh processes and are excluded from summaries. Result checkpoints occur outside timed launches.',
      persistence: 'Atomic results.json replacement after each trial/workload; status remains running until final provenance checks finish.',
      percentile: 'Nearest rank: sorted[ceil(0.95 * N) - 1]. With fewer than 20 samples p95 is the maximum.',
      solxLimits: 'Default interpreter limits; no unlimited-steps or increased-depth flags.',
    },
    config: { samples: config.samples, warmups: config.warmups, timeoutMs: config.timeoutMs },
    solxBuild: { profileLabel: config.buildProfile, command: config.buildCommand ?? null, labelIsUserSupplied: true },
    environment: {
      os: { platform: process.platform, release: os.release(), version: os.version(), architecture: os.arch() },
      cpu: { model: os.cpus()[0]?.model ?? null, logicalProcessors: os.cpus().length },
      memoryBytes: os.totalmem(), node: process.version,
      runtimeEnvironment: {
        policy: 'Clear LUA_INIT/LUA_PATH/LUA_CPATH and all version-suffixed variants for runtime launches and version probes; inherit other variables without recording their values.',
        removedNames: removedEnvironmentNames,
        imports: 'Lua/LuaJIT search only the helper fixture directory; Luau uses a relative helper path.',
      },
      rustc: execute('rustc', ['--version'], 5000), cargo: execute('cargo', ['--version'], 5000),
    },
    repositoryBefore: repositorySnapshot(), repositoryAfter: null,
    harnessSha256: digest(__filename), manifestSha256: digest(manifestFile), engines: selected, cases,
    trials: [], summaries: [],
  };
  const output = path.join(directory, 'results.json');
  const pendingOutput = path.join(directory, 'results.json.next');
  const persist = () => {
    fs.writeFileSync(pendingOutput, `${JSON.stringify(report, null, 2)}\n`);
    fs.renameSync(pendingOutput, output);
  };
  let failure = null;
  try {
    persist();
    for (const workload of cases) {
      console.log(`Measuring ${workload.name} (${selected.map(engine => engine.name).join(', ')})`);
      for (const phase of ['warmup', 'sample']) {
        const count = phase === 'warmup' ? config.warmups : config.samples;
        for (let index = 0; index < count; index += 1) {
          for (let offset = 0; offset < selected.length; offset += 1) {
            const engine = selected[(index + offset) % selected.length];
            const source = (workload.files[engine.name] || workload.files.lua).path;
            const trial = {
              workload: workload.name, engine: engine.name, phase, index,
              ...execute(engine.executable, [source], config.timeoutMs, runtimeEnvironment),
            };
            trial.valid = !trial.error && trial.exitCode === 0 && trial.signal === null
              && !trial.stdoutTruncated && !trial.stderrTruncated
              && trial.stdout.trim() === workload.expectedStdout && trial.stderr.length === 0;
            report.trials.push(trial);
            persist();
            if (!trial.valid) throw new Error(`${engine.name}/${workload.name} failed exit, output, or timeout validation; see raw trial`);
          }
        }
      }
      for (const engine of selected) {
        const values = report.trials.filter(trial => trial.phase === 'sample'
          && trial.workload === workload.name && trial.engine === engine.name).map(trial => trial.elapsedMs);
        const result = { workload: workload.name, kind: workload.kind, engine: engine.name, ...summary(values) };
        report.summaries.push(result);
        console.log(`  ${engine.name}: median ${result.medianMs.toFixed(3)} ms; p95 ${result.p95Ms.toFixed(3)} ms`);
      }
      persist();
    }
    report.repositoryAfter = repositorySnapshot();
    report.runtimeSourcesUnchanged = JSON.stringify(report.repositoryBefore.sourceSha256)
      === JSON.stringify(report.repositoryAfter.sourceSha256);
    for (const engine of selected) {
      engine.executableSha256After = digest(engine.executable);
      if (engine.executableSha256 !== engine.executableSha256After) {
        throw new Error(`${engine.name} executable changed during measurement`);
      }
    }
    if (!report.runtimeSourcesUnchanged) throw new Error('Runtime source files changed during measurement');
    if (digest(__filename) !== report.harnessSha256 || digest(manifestFile) !== report.manifestSha256) {
      throw new Error('Benchmark harness or manifest changed during measurement');
    }
    for (const workload of cases) {
      for (const file of [...Object.values(workload.files), ...workload.supportFiles]) {
        if (digest(file.path) !== file.sha256) throw new Error('Benchmark fixture changed during measurement');
      }
    }
    report.status = 'complete';
  } catch (error) {
    failure = error;
    report.status = 'failed';
    report.failure = error.message;
  } finally {
    report.finishedAt = new Date().toISOString();
    persist();
    console.log(`Raw results: ${output}`);
  }
  if (failure) throw failure;
}

try {
  const config = options(process.argv.slice(2));
  if (config.help) process.stdout.write(help);
  else main(config);
} catch (error) {
  console.error(`benchmark: ${error.message}`);
  process.exitCode = 1;
}
