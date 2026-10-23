const vscode = require('vscode');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

exports.run = async function () {
  const root = path.resolve(__dirname, '../..');
  const workspace = vscode.workspace.workspaceFolders[0].uri.fsPath;
  const report = { vscode: vscode.version, checks: [], success: false };
  const record = () => fs.writeFileSync(process.env.SOLX_HOST_RESULTS, JSON.stringify(report, null, 2));
  const check = async (name, action) => { await action(); report.checks.push(name); record(); };
  const open = async (name, source) => {
    const file = path.join(workspace, name);
    fs.writeFileSync(file, source);
    const document = await vscode.workspace.openTextDocument(file);
    await vscode.window.showTextDocument(document);
    return document;
  };
  const configure = async (key, value) => vscode.workspace.getConfiguration('solx').update(key, value, vscode.ConfigurationTarget.Workspace);
  const complete = (doc, offset) => vscode.commands.executeCommand('vscode.executeCompletionItemProvider', doc.uri, doc.positionAt(offset));
  const hovers = (doc, offset) => vscode.commands.executeCommand('vscode.executeHoverProvider', doc.uri, doc.positionAt(offset));
  const label = item => typeof item.label === 'string' ? item.label : item.label.label;
  // VS Code also returns its own word-based Text suggestions. The Solx
  // provider contributes typed Keyword/Function/Module/Snippet items only.
  const solxItems = result => (result?.items || []).filter(item => item.kind !== vscode.CompletionItemKind.Text);
  try {
    let doc;
    await check('activation and .solx language association', async () => {
      doc = await open('valid & spaces.solx', 'import "math" as m\nvar answer = m.sqrt(25)\nprint(answer)\n');
      assert.equal(doc.languageId, 'solx');
      const extensionPath = process.env.SOLX_EXTENSION_PATH || root;
      const extension = vscode.extensions.all.find(item => path.resolve(item.extensionPath).toLowerCase() === extensionPath.toLowerCase());
      assert.ok(extension);
      await extension.activate();
      assert.equal(extension.isActive, true);
      report.extension = extension.id;
    });
    await check('member completion from an imported alias', async () => {
      const items = await complete(doc, doc.getText().indexOf('sqrt') + 2);
      assert.ok(items.items.some(item => label(item) === 'sqrt' && item.kind === vscode.CompletionItemKind.Function));
    });
    await check('hover content and source range', async () => {
      const items = await hovers(doc, doc.getText().indexOf('sqrt') + 1);
      assert.ok(items.some(item => item.contents.some(content => content.value.includes('math.sqrt(x)'))));
      assert.equal(doc.getText(items[0].range), 'sqrt');
    });
    await check('negative completion and hover contexts', async () => {
      const negative = await open('negative.solx', '-- print\nvar text = "print"\nvar object = {}\nobject.\n');
      for (const offset of [8, negative.getText().indexOf('print"') + 3, negative.getText().indexOf('object.\n') + 7]) {
        const items = solxItems(await complete(negative, offset));
        assert.equal(items.length, 0, JSON.stringify({ offset, items: items.map(item => ({ label: label(item), detail: item.detail, kind: item.kind })) }));
      }
      assert.equal((await hovers(negative, negative.getText().indexOf('print"') + 1)).length, 0);
    });
    await check('snippet insertion uses a real VS Code SnippetString', async () => {
      const snippetDoc = await open('snippet.solx', 'fn');
      const items = await complete(snippetDoc, 2);
      const item = items.items.find(item => label(item) === 'fn' && item.kind === vscode.CompletionItemKind.Snippet);
      assert.ok(item);
      await vscode.window.activeTextEditor.insertSnippet(item.insertText, item.range);
      assert.match(snippetDoc.getText(), /^fn name\(argument\)\r?\n/);
      await snippetDoc.save();
    });
    await check('run current file through Cargo with a fresh build', async () => {
      await configure('cargoManifestPath', path.join(root, '../Cargo.toml'));
      await vscode.window.showTextDocument(doc);
      const result = await vscode.commands.executeCommand('solx.runCurrentFile');
      assert.equal(result.status, 'success', JSON.stringify(result));
    });
    await check('nonzero parser exit is reported by the command', async () => {
      await open('invalid.solx', 'var broken = )');
      const result = await vscode.commands.executeCommand('solx.runCurrentFile');
      assert.equal(result.status, 'failed');
      assert.equal(result.code, 1);
    });
    await check('missing interpreter is handled in the actual host', async () => {
      await configure('cargoManifestPath', '');
      await configure('executablePath', path.join(workspace, 'missing-solx.exe'));
      const result = await vscode.commands.executeCommand('solx.runCurrentFile');
      assert.equal(result.status, 'failed');
      assert.match(result.message, /Executable not found/);
      await configure('executablePath', '');
    });
    await check('untitled files cannot be run', async () => {
      const untitled = await vscode.workspace.openTextDocument({ language: 'solx', content: 'print(1)' });
      await vscode.window.showTextDocument(untitled);
      assert.equal((await vscode.commands.executeCommand('solx.runCurrentFile')).status, 'failed');
    });
    await check('stop command terminates a running Solx program', async () => {
      await configure('cargoManifestPath', path.join(root, '../Cargo.toml'));
      await open('loop.solx', 'while true do end');
      let finished = false;
      const running = vscode.commands.executeCommand('solx.runCurrentFile').then(result => { finished = true; return result; });
      for (let i = 0; i < 100 && !finished; i++) {
        await new Promise(resolve => setTimeout(resolve, 100));
        await vscode.commands.executeCommand('solx.stopRun');
      }
      assert.equal(finished, true, 'Run did not stop within ten seconds');
      assert.equal((await running).status, 'cancelled');
    });
    await check('completion and hover settings take effect', async () => {
      await configure('completion.enabled', false);
      await configure('hover.enabled', false);
      assert.equal(solxItems(await complete(doc, doc.getText().indexOf('sqrt') + 2)).length, 0);
      assert.equal((await hovers(doc, doc.getText().indexOf('sqrt') + 1)).length, 0);
    });
    report.success = true;
  } catch (error) {
    report.error = error.stack;
    throw error;
  } finally { record(); }
};
