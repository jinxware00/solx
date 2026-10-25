const vscode = require('vscode');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

exports.run = async function () {
  const root = path.resolve(__dirname, '../..');
  const workspace = vscode.workspace.workspaceFolders[0].uri.fsPath;
  const report = { vscode: vscode.version, checks: [], timings: {}, success: false };
  const record = () => fs.writeFileSync(process.env.SOLX_HOST_RESULTS, JSON.stringify(report, null, 2));
  const check = async (name, action) => {
    report.currentCheck = name; record();
    const started = Date.now();
    await action();
    report.timings[name] = Date.now() - started;
    report.checks.push(name); delete report.currentCheck; record();
  };
  const open = async (name, source) => {
    const file = path.join(workspace, name);
    fs.writeFileSync(file, source);
    const document = await vscode.workspace.openTextDocument(file);
    await vscode.window.showTextDocument(document);
    return document;
  };
  const configure = async (key, value) => {
    const scope = vscode.workspace.workspaceFolders[0].uri;
    const configuration = vscode.workspace.getConfiguration('solx', scope);
    if (configuration.get(key) === value) return;
    let subscription, timeout, rejectChange;
    const changed = new Promise((resolve, reject) => {
      rejectChange = reject;
      subscription = vscode.workspace.onDidChangeConfiguration(event => {
        if (event.affectsConfiguration(`solx.${key}`, scope)
          && vscode.workspace.getConfiguration('solx', scope).get(key) === value) resolve();
      });
    });
    try {
      await configuration.update(key, value, vscode.ConfigurationTarget.Workspace);
      // File saves and save participants belong to update(); start the event
      // delivery deadline only after that operation actually finishes.
      timeout = setTimeout(() => rejectChange(new Error(`Configuration event missing for solx.${key}`)), 5000);
      await changed;
    }
    finally { clearTimeout(timeout); subscription.dispose(); }
  };
  const complete = (doc, offset) => vscode.commands.executeCommand('vscode.executeCompletionItemProvider', doc.uri, doc.positionAt(offset));
  const hovers = (doc, offset) => vscode.commands.executeCommand('vscode.executeHoverProvider', doc.uri, doc.positionAt(offset));
  const documentSymbols = async doc => {
    const result = await vscode.commands.executeCommand('vscode.executeDocumentSymbolProvider', doc.uri);
    // VS Code 1.85's command adapter returns undefined for an empty symbol
    // array, including when a registered provider deliberately returns [].
    assert.ok(result === undefined || Array.isArray(result), 'Unexpected document symbol result');
    return result ?? [];
  };
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
    await check('live diagnostics use the parser and exact Unicode/CRLF ranges', async () => {
      const source = "var text='😀é'\r\nvar broken=)";
      const checked = await open('syntax diagnostics.solx', source);
      const result = await vscode.commands.executeCommand('solx.checkCurrentFile');
      assert.equal(result.status, 'checked', JSON.stringify(result));
      assert.equal(result.diagnostics, 1);
      const [diagnostic] = vscode.languages.getDiagnostics(checked.uri).filter(item => item.source === 'Solx');
      assert.equal(diagnostic.code, 'parse-error');
      assert.equal(diagnostic.range.start.line, 1);
      assert.equal(checked.getText(diagnostic.range), ')');
    });
    await check('unsaved edits are checked automatically and never executed', async () => {
      const checked = await open('unsaved diagnostics.solx', 'var x=1');
      const editor = vscode.window.activeTextEditor;
      await editor.edit(edit => edit.replace(new vscode.Range(checked.positionAt(0), checked.positionAt(checked.getText().length)), 'var text="😀"\r\n@'));
      assert.ok(checked.isDirty);
      // Allow the production check timeout plus debounce and host scheduling.
      const deadline = Date.now() + 35000;
      let items;
      do {
        items = vscode.languages.getDiagnostics(checked.uri).filter(item => item.source === 'Solx');
        if (items.length) break;
        await new Promise(resolve => setTimeout(resolve, 50));
      } while (Date.now() < deadline);
      assert.equal(items.length, 1, 'No automatic diagnostic appeared after editing');
      assert.equal(items[0].code, 'lex-error');
      assert.equal(checked.getText(items[0].range), '@');
      assert.equal(fs.readFileSync(checked.uri.fsPath, 'utf8'), 'var x=1');
      await editor.edit(edit => edit.replace(new vscode.Range(checked.positionAt(0), checked.positionAt(checked.getText().length)), "while true do end import './missing.solx'"));
      const result = await vscode.commands.executeCommand('solx.checkCurrentFile');
      assert.equal(result.status, 'checked', JSON.stringify(result));
      assert.equal(result.diagnostics, 0);
      assert.equal(vscode.languages.getDiagnostics(checked.uri).filter(item => item.source === 'Solx').length, 0);
    });
    await check('untitled diagnostics and disable/enable configuration', async () => {
      const checked = await vscode.workspace.openTextDocument({ language: 'solx', content: 'var x=)' });
      await vscode.window.showTextDocument(checked);
      assert.equal((await vscode.commands.executeCommand('solx.checkCurrentFile')).diagnostics, 1);
      await configure('diagnostics.enabled', false);
      assert.equal((await vscode.commands.executeCommand('solx.checkCurrentFile')).status, 'skipped');
      assert.equal(vscode.languages.getDiagnostics(checked.uri).filter(item => item.source === 'Solx').length, 0);
      await configure('diagnostics.enabled', true);
      assert.equal((await vscode.commands.executeCommand('solx.checkCurrentFile')).diagnostics, 1);
    });
    await check('repeated warm checks avoid the Windows startup stall', async () => {
      const document = await open("check speed & quote'.solx", 'var x=1');
      const editor = vscode.window.activeTextEditor;
      report.checkDurationsMs = [];
      for (const [source, expected] of [['var x=)', 1], ['var x=1', 0], ['while true do end', 0]]) {
        await editor.edit(edit => edit.replace(new vscode.Range(document.positionAt(0), document.positionAt(document.getText().length)), source));
        const started = Date.now();
        const result = await vscode.commands.executeCommand('solx.checkCurrentFile');
        const elapsed = Date.now() - started;
        report.checkDurationsMs.push(elapsed); record();
        assert.equal(result.status, 'checked', JSON.stringify(result));
        assert.equal(result.diagnostics, expected);
        assert.ok(elapsed < 10000, `Warm syntax check took ${elapsed} ms; possible startup regression`);
      }
      assert.equal(fs.readFileSync(document.uri.fsPath, 'utf8'), 'var x=1');
    });
    await check('nonzero parser exit is reported by the command', async () => {
      await open('invalid.solx', 'var broken = )');
      const result = await vscode.commands.executeCommand('solx.runCurrentFile');
      assert.equal(result.status, 'failed');
      assert.equal(result.code, 1);
    });
    await check('Outline uses real parser declaration names and source selections', async () => {
      const source = 'var text="😀"\r\nimport "math" as m\r\nfn greet(name) return name end\r\n';
      const outlined = await open('outline.solx', source);
      const version = outlined.version;
      const symbols = await documentSymbols(outlined);
      assert.deepEqual(symbols.map(item => item.name), ['text', 'm', 'greet']);
      assert.deepEqual(symbols.map(item => outlined.getText(item.selectionRange)), ['text', 'm', 'greet']);
      assert.equal(symbols[2].kind, vscode.SymbolKind.Function);
      await configure('symbols.enabled', false);
      let disabled;
      const deadline = Date.now() + 5000;
      do {
        disabled = await documentSymbols(outlined);
        if (disabled.length === 0) break;
        await new Promise(resolve => setTimeout(resolve, 50));
      } while (Date.now() < deadline);
      assert.equal(disabled.length, 0);
      assert.deepEqual(await documentSymbols(outlined), []);
      await configure('symbols.enabled', true);
      let restored;
      const restoredDeadline = Date.now() + 35000;
      do {
        restored = await documentSymbols(outlined);
        if (restored.length) break;
        await new Promise(resolve => setTimeout(resolve, 50));
      } while (Date.now() < restoredDeadline);
      assert.deepEqual(restored.map(item => item.name), ['text', 'm', 'greet']);
      assert.deepEqual(restored.map(item => outlined.getText(item.selectionRange)), ['text', 'm', 'greet']);
      assert.equal(outlined.version, version, 'Configuration refresh must not edit the document');
    });
    await check('optional undefined-name diagnostics are real, ranged, and disabled by default', async () => {
      const checked = await open('name checks.solx', 'var text="😀"\r\nprint(missing)');
      assert.equal((await vscode.commands.executeCommand('solx.checkCurrentFile')).diagnostics, 0);
      await configure('diagnostics.checkNames', true);
      assert.equal((await vscode.commands.executeCommand('solx.checkCurrentFile')).diagnostics, 1);
      const [diagnostic] = vscode.languages.getDiagnostics(checked.uri).filter(item => item.source === 'Solx');
      assert.equal(diagnostic.code, 'undefined-name');
      assert.equal(checked.getText(diagnostic.range), 'missing');
      await configure('diagnostics.checkNames', false);
    });
    await check('raw multiline strings suppress Solx completion and hover', async () => {
      const raw = await open('raw strings.solx', 'var text="""first\nprint\nlast"""');
      assert.equal(solxItems(await complete(raw, raw.getText().indexOf('print') + 3)).length, 0);
      assert.equal((await hovers(raw, raw.getText().indexOf('print') + 1)).length, 0);
    });
    await check('missing interpreter is handled in the actual host', async () => {
      await configure('cargoManifestPath', '');
      await configure('executablePath', path.join(workspace, 'missing-solx.exe'));
      const result = await vscode.commands.executeCommand('solx.runCurrentFile');
      assert.equal(result.status, 'failed');
      assert.match(result.message, /Executable not found/);
      const checked = await vscode.commands.executeCommand('solx.checkCurrentFile');
      assert.equal(checked.status, 'failed');
      assert.equal(vscode.languages.getDiagnostics(vscode.window.activeTextEditor.document.uri).filter(item => item.source === 'Solx').length, 0);
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
