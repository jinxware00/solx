const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');

// Use an exclusively created file in a fresh directory under the user's temp
// directory. Never recursively remove a directory or touch the saved document.
async function createSnapshot(source, { io = fs, directory = os.tmpdir(), onCleanupError = () => {} } = {}) {
  const folder = await io.mkdtemp(path.join(directory, 'solx-check-'));
  const file = path.join(folder, 'buffer.solx');
  let disposal;
  const dispose = () => disposal ||= (async () => {
    try {
      try { await io.unlink(file); } catch (error) { if (error.code !== 'ENOENT') throw error; }
      try { await io.rmdir(folder); } catch (error) { if (error.code !== 'ENOENT') throw error; }
    } catch (error) {
      const failure = new Error(`Could not remove Solx syntax-check snapshot at ${folder}: ${error.message}`);
      try { onCleanupError(failure); } catch { /* Reporting must not replace the cleanup failure. */ }
      throw failure;
    }
  })();
  try {
    await io.writeFile(file, source, { encoding: 'utf8', flag: 'wx', mode: 0o600 });
    return { file, dispose };
  } catch (error) {
    try { await dispose(); } catch (cleanup) {
      throw new Error(`Could not write Solx syntax-check snapshot: ${error.message}. ${cleanup.message}`);
    }
    throw new Error(`Could not write Solx syntax-check snapshot: ${error.message}`);
  }
}

module.exports = { createSnapshot };
