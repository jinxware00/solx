const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const { createSnapshot } = require('../src/snapshot.cjs');

async function fixture(t) {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'solx-snapshot-test-'));
  t.after(() => fs.rmdir(directory));
  return directory;
}

test('snapshots preserve exact UTF-8 bytes, use unique directories, and clean up idempotently', async t => {
  const directory = await fixture(t);
  const source = "var text='😀é'\r\nvar x=)";
  const first = await createSnapshot(source, { directory });
  const second = await createSnapshot(source, { directory });
  assert.notEqual(first.file, second.file);
  for (const snapshot of [first, second]) {
    assert.equal(path.dirname(path.dirname(snapshot.file)), directory);
    assert.deepEqual(await fs.readFile(snapshot.file), Buffer.from(source, 'utf8'));
    if (process.platform !== 'win32') {
      assert.equal((await fs.stat(snapshot.file)).mode & 0o777, 0o600);
      assert.equal((await fs.stat(path.dirname(snapshot.file))).mode & 0o777, 0o700);
    }
    await Promise.all([snapshot.dispose(), snapshot.dispose()]);
  }
  assert.deepEqual(await fs.readdir(directory), []);
});

test('a partial snapshot write failure removes the owned file and directory', async t => {
  const directory = await fixture(t);
  const io = { ...fs, async writeFile(file, source, options) {
    assert.equal(options.flag, 'wx');
    await fs.writeFile(file, source.slice(0, 3), options);
    throw new Error('disk full');
  } };
  await assert.rejects(createSnapshot('var x=1', { directory, io }), /Could not write.*disk full/);
  assert.deepEqual(await fs.readdir(directory), []);
});

test('cleanup reports its location and preserves unexpected files instead of deleting recursively', async t => {
  const directory = await fixture(t);
  const reported = [];
  const snapshot = await createSnapshot('var x=1', { directory, onCleanupError: error => reported.push(error.message) });
  const folder = path.dirname(snapshot.file);
  const unexpected = path.join(folder, 'preserve.txt');
  await fs.writeFile(unexpected, 'preserve me');
  await assert.rejects(snapshot.dispose(), error => error.message.includes(folder) && /Could not remove/.test(error.message));
  await assert.rejects(snapshot.dispose());
  assert.equal(reported.length, 1);
  assert.ok(reported[0].includes(folder));
  assert.equal(await fs.readFile(unexpected, 'utf8'), 'preserve me');
  await assert.rejects(fs.stat(snapshot.file), { code: 'ENOENT' });
  // Remove only the files/directories created by this test.
  await fs.unlink(unexpected);
  await fs.rmdir(folder);
});
