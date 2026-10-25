const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '..');
const json = name => JSON.parse(fs.readFileSync(path.join(root, name), 'utf8'));

test('JSON resources, entry point, icon, and exact development dependencies are valid', () => {
  const pkg = json('package.json');
  const lock = json('package-lock.json');
  for (const file of ['language-configuration.json', 'syntaxes/solx.tmLanguage.json', '.vscode/launch.json', 'data/snippets.json']) json(file);
  assert.equal(pkg.name, 'solx-ls');
  assert.equal(pkg.displayName, 'Sol Language Support');
  assert.equal(pkg.publisher, 'jinxware00');
  assert.equal(pkg.author.url, 'https://github.com/jinxware00');
  assert.equal(lock.name, pkg.name);
  assert.equal(lock.version, pkg.version);
  assert.equal(lock.packages[''].name, pkg.name);
  assert.equal(lock.packages[''].version, pkg.version);
  assert.deepEqual(lock.packages[''].devDependencies, pkg.devDependencies);
  assert.ok(Object.values(pkg.devDependencies).every(version => /^\d+\.\d+\.\d+$/.test(version)));
  assert.equal(pkg.dependencies, undefined);
  assert.ok(fs.existsSync(path.join(root, pkg.main)));
  const png = fs.readFileSync(path.join(root, pkg.icon));
  assert.equal(png.subarray(0, 8).toString('hex'), '89504e470d0a1a0a');
  assert.equal(png.readUInt32BE(16), 512);
  assert.equal(png.readUInt32BE(20), 512);
  assert.ok(pkg.files.includes(pkg.icon));
  assert.ok(pkg.files.includes('src/**'));
  assert.equal(pkg.capabilities.untrustedWorkspaces.supported, 'limited');
  assert.ok(pkg.contributes.commands.some(command => command.command === 'solx.runCurrentFile'));
});

test('README uses real local images and names only implemented features', () => {
  const readme = fs.readFileSync(path.join(root, 'README.md'), 'utf8');
  const images = [...readme.matchAll(/<img\b[^>]*src="([^"]+)"|!\[[^\]]*\]\(([^)]+)\)/g)];
  assert.ok(images.length > 0);
  for (const match of images) {
    const asset = match[1] || match[2];
    assert.ok(!/^https?:/.test(asset));
    assert.ok(fs.existsSync(path.join(root, asset)), asset);
  }
  assert.ok(readme.includes(`${json('package.json').name}-${json('package.json').version}.vsix`));
  assert.match(readme, /parse-only/);
});
