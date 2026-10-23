const { startProcess } = require('./runner.cjs');
const { createSnapshot } = require('./snapshot.cjs');

const MAX_SOURCE_BYTES = 1_048_576;
const MAX_OUTPUT = 1_048_576;

// Convert only validated UTF-8 boundaries, using the same encoding as stdin.
// JS offsets remain UTF-16; TextDocument.positionAt handles CRLF and lone CR.
function decodeReport(source, stdout, code) {
  let report;
  try { report = JSON.parse(stdout); } catch { throw new Error('No valid check response. Rebuild or update Solx to a version supporting check --json.'); }
  if (report?.schemaVersion !== 1 || report.offsetEncoding !== 'utf-8' || typeof report.source !== 'string'
      || typeof report.ok !== 'boolean' || !Array.isArray(report.diagnostics) || report.diagnostics.length > 100) {
    throw new Error('Unsupported Solx check protocol. Rebuild or update the interpreter and extension together.');
  }
  if (report.error !== null) {
    if (code !== 2 || report.ok || report.diagnostics.length || report.error?.code !== 'input-error' || typeof report.error.message !== 'string') {
      throw new Error('Invalid Solx check failure response.');
    }
    throw new Error(report.error.message);
  }
  if ((report.ok && (code !== 0 || report.diagnostics.length !== 0))
      || (!report.ok && (code !== 1 || report.diagnostics.length === 0))) throw new Error('Solx check result and exit code disagree.');
  const needed = new Set(report.diagnostics.flatMap(item => [item.range?.start, item.range?.end]));
  if (!needed.size) return [];
  const boundaries = new Map([[0, 0]]);
  let bytes = 0, offset = 0;
  for (const character of source) {
    bytes += Buffer.byteLength(character, 'utf8');
    offset += character.length;
    if (needed.has(bytes)) boundaries.set(bytes, offset);
  }
  return report.diagnostics.map(item => {
    const { start, end } = item.range || {};
    if (item.severity !== 'error' || !['lex-error', 'parse-error', 'syntax-limit'].includes(item.code)
        || typeof item.message !== 'string' || !item.message.length || item.message.length > 65536
        || !Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start > end
        || !boundaries.has(start) || !boundaries.has(end)) throw new Error('Invalid diagnostic range or content from Solx check.');
    return { code: item.code, message: item.message, start: boundaries.get(start), end: boundaries.get(end) };
  });
}

function startCheck(launch, source, { start = startProcess, snapshot: prepare = createSnapshot,
  platform = process.platform, timeoutMs = 30000, onCleanupError, ...dependencies } = {}) {
  if (Buffer.byteLength(source, 'utf8') > MAX_SOURCE_BYTES) throw new Error('Solx checking is limited to 1 MiB of UTF-8 source.');
  let stdout = '', stderr = '', timedOut = false, stopped = false, run, snapshot, disposal, timeout;
  const dispose = () => snapshot && (disposal ||= snapshot.dispose());
  const work = (async () => {
    try {
      // File checks avoid the measured Windows startup stall in stdin mode.
      // Keep exact buffer bytes and the same non-executing JSON protocol.
      if (platform === 'win32') snapshot = await prepare(source, { onCleanupError });
      if (stopped || timedOut) return { cancelled: true };
      const args = [...launch.args, ...(snapshot ? ['--', snapshot.file] : ['--stdin'])];
      run = start({ ...launch, args }, () => {}, { ...dependencies, platform,
        input: snapshot ? undefined : source, maxOutput: MAX_OUTPUT,
        onStdout: text => { stdout += text; }, onStderr: text => { stderr += text; } });
      const result = await run.completion;
      return stopped ? { ...result, cancelled: true } : result;
    } finally { await dispose(); }
  })();
  const deadline = new Promise((_resolve, reject) => {
    timeout = setTimeout(() => {
      timedOut = true;
      try { run?.stop(); } catch { /* Still settle the request if termination fails. */ }
      reject(new Error(`Solx check timed out after ${timeoutMs} ms. Check the interpreter configuration or increase solx.diagnostics.timeoutMs for a cold Cargo build.`));
    }, timeoutMs);
  });
  const completion = Promise.race([work, deadline]).then(result => {
    if (timedOut) throw new Error(`Solx check timed out after ${timeoutMs} ms. Check the interpreter configuration or increase solx.diagnostics.timeoutMs for a cold Cargo build.`);
    if (result.cancelled) return { cancelled: true };
    if (result.error) throw new Error(`Could not launch Solx check: ${result.error.message}`);
    if (result.truncated) throw new Error('Solx check exceeded its output limit.');
    // Prefer a useful old-CLI/protocol error over EPIPE caused by its early exit.
    let diagnostics;
    try { diagnostics = decodeReport(source, stdout, result.code); }
    catch (error) { throw new Error(`${error.message}${stderr.trim() ? `\n${stderr.trim().slice(0, 2000)}` : ''}`); }
    if (result.inputError) throw new Error(`Could not send source to Solx check: ${result.inputError.message}`);
    return { diagnostics };
  }).finally(async () => { clearTimeout(timeout); await dispose(); });
  return { completion, stop() { stopped = true; run?.stop(); } };
}

module.exports = { MAX_SOURCE_BYTES, decodeReport, startCheck };
