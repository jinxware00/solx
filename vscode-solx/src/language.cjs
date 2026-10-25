const { builtins, modules, keywords, types } = require('./catalog.cjs');
const snippets = require('../data/snippets.json');
const own = (object, key) => Object.prototype.hasOwnProperty.call(object, key);

// A tolerant lexical context scanner, not a parser or diagnostic engine.
// Offsets are UTF-16, matching VS Code. Invalid input always makes progress.
function scan(source) {
  const tokens = [];
  let i = 0;
  while (i < source.length) {
    if (/\s/.test(source[i])) { i++; continue; }
    const start = i;
    let kind = 'punctuation', closed = true;
    if (source.startsWith('--', i)) {
      kind = 'comment';
      while (i < source.length && !/[\r\n]/.test(source[i])) i++;
    } else if (source.startsWith('"""', i)) {
      kind = 'string';
      const end = source.indexOf('"""', i + 3);
      closed = end !== -1;
      i = closed ? end + 3 : source.length;
    } else if (source[i] === '"' || source[i] === "'") {
      kind = 'string';
      const quote = source[i++];
      closed = false;
      while (i < source.length && !/[\r\n]/.test(source[i])) {
        if (source[i] === quote) { i++; closed = true; break; }
        if (source[i] === '\\' && i + 1 < source.length && !/[\r\n]/.test(source[i + 1])) i++;
        i++;
      }
    } else if (/[A-Za-z_]/.test(source[i])) {
      kind = 'identifier';
      while (i < source.length && /[A-Za-z0-9_]/.test(source[i])) i++;
    } else if (/[0-9]/.test(source[i]) || (source[i] === '.' && /[0-9]/.test(source[i + 1] || ''))) {
      kind = 'number';
      const number = /^(?:[0-9]+(?:\.[0-9]+)?|\.[0-9]+)(?:[eE][+-]?[0-9]*)?/.exec(source.slice(i));
      i += number[0].length;
    } else {
      i += ['..', '//', '==', '~=', '<=', '>='].includes(source.slice(i, i + 2)) ? 2 : 1;
    }
    tokens.push({ text: source.slice(start, i), kind, start, end: i, closed });
  }
  return tokens;
}

function inLiteral(tokens, offset) {
  return tokens.some(t => (t.kind === 'comment' || t.kind === 'string') && t.start <= offset &&
    (offset < t.end || (offset === t.end && (t.kind === 'comment' || !t.closed))));
}

// Resolve direct lexical bindings only. Assignments invalidate inferred APIs;
// we do not infer arbitrary values, follow file imports, or execute any code.
function bindingsAt(tokens, offset) {
  const scopes = [new Map(Object.keys(builtins).map(name => [name, { builtin: name }]))];
  const current = () => scopes[scopes.length - 1];
  const text = i => tokens[i]?.text;
  for (let i = 0; i < tokens.length && tokens[i].start < offset; i++) {
    const token = tokens[i];
    if (token.kind !== 'identifier') continue;
    if (token.text === 'fn') {
      let p = i + 1;
      if (tokens[p]?.kind === 'identifier') current().set(text(p++), null);
      scopes.push(new Map());
      if (text(p) === '(') {
        while (++p < tokens.length && text(p) !== ')') {
          if (tokens[p].kind === 'identifier') current().set(text(p), null);
        }
      }
    } else if (['if', 'for', 'while', 'try'].includes(token.text)) {
      scopes.push(new Map());
      if (token.text === 'for' && tokens[i + 1]?.kind === 'identifier') {
        current().set(text(i + 1), null);
        if (text(i + 2) === ',' && tokens[i + 3]?.kind === 'identifier' && text(i + 4) === 'in') current().set(text(i + 3), null);
      }
    } else if (['else', 'elseif', 'catch'].includes(token.text) && scopes.length > 1) {
      scopes[scopes.length - 1] = new Map();
      if (token.text === 'catch' && tokens[i + 1]?.kind === 'identifier') current().set(text(i + 1), null);
    } else if (token.text === 'end' && scopes.length > 1) {
      scopes.pop();
    } else if (['var', 'const', 'global'].includes(token.text) && tokens[i + 1]?.kind === 'identifier') {
      // Suppress documentation for user bindings even when declared global.
      (token.text === 'global' ? scopes[0] : current()).set(text(i + 1), null);
    } else if (token.text === 'import' && tokens[i + 1]?.kind === 'string' && tokens[i + 1].closed) {
      const literal = text(i + 1);
      const delimiterLength = literal.startsWith('"""') ? 3 : 1;
      const specifier = literal.slice(delimiterLength, -delimiterLength);
      const alias = text(i + 2) === 'as' ? tokens[i + 3] : undefined;
      const name = alias?.kind === 'identifier' ? alias.text : specifier.split(/[\\/]/).pop().replace(/\.solx$/, '');
      current().set(name, own(modules, specifier) ? { module: specifier } : null);
    } else if (text(i + 1) === '=' && !['.', '{', ','].includes(text(i - 1))) {
      for (let s = scopes.length - 1; s >= 0; s--) {
        if (scopes[s].has(token.text)) { scopes[s].set(token.text, null); break; }
      }
    }
  }
  return new Map(scopes.flatMap(scope => [...scope]));
}

function context(source, offset) {
  // Keep editor requests bounded for generated/very large documents.
  if (source.length > 1_000_000) return undefined;
  const all = scan(source);
  if (inLiteral(all, offset)) return undefined;
  return all.filter(t => t.kind !== 'comment');
}

function receiver(tokens, dotIndex) {
  const name = tokens[dotIndex - 1];
  if (name?.kind !== 'identifier' || ['.', ']', ')'].includes(tokens[dotIndex - 2]?.text)) return undefined;
  return name.text;
}

function completions(source, offset) {
  const tokens = context(source, offset);
  if (!tokens) return [];
  const prefix = /[A-Za-z_][A-Za-z0-9_]*$/.exec(source.slice(0, offset))?.[0] || '';
  const start = offset - prefix.length;
  const preceding = tokens.filter(t => t.end <= start);
  const previous = preceding[preceding.length - 1];
  const bindings = bindingsAt(tokens, start);
  let entries;
  if (previous?.text === '.') {
    const name = receiver(preceding, preceding.length - 1);
    const module = bindings.get(name)?.module;
    entries = module ? Object.entries(modules[module]).map(([label, data]) => ({ label, ...data })) : [];
  } else if (previous?.text === ':') {
    entries = ['var', 'global', 'const'].includes(preceding[preceding.length - 3]?.text)
      ? types.map(label => ({ label, kind: 'TypeParameter', description: 'Variable type annotation, checked at runtime.' })) : [];
  } else if (previous?.kind === 'string' && preceding[preceding.length - 2]?.text === 'import') {
    entries = [{ label: 'as', kind: 'Keyword', description: 'Bind this import under a local alias.' }];
  } else if (['var', 'global', 'const', 'fn', 'for', 'as', 'catch'].includes(previous?.text)) {
    entries = [];
  } else {
    entries = Object.entries(keywords).map(([label, description]) => ({ label, description, kind: 'Keyword' }));
    for (const [label, binding] of bindings) {
      if (binding?.builtin) entries.push({ label, ...builtins[binding.builtin] });
      if (binding?.module) entries.push({ label, kind: 'Module', description: `Imported ${binding.module} standard module.` });
    }
    entries.push(...Object.entries(snippets).map(([name, snippet]) => ({
      label: snippet.prefix, kind: 'Snippet', description: snippet.description,
      detail: name, insertText: snippet.body.join('\n'),
    })));
  }
  return entries.filter(item => item.label.startsWith(prefix)).map(item => ({ ...item, start, end: offset }));
}

function hover(source, offset) {
  const tokens = context(source, offset);
  if (!tokens) return undefined;
  const index = tokens.findIndex(t => t.start <= offset && offset < t.end);
  const token = tokens[index];
  if (token?.kind !== 'identifier' || tokens[index + 1]?.text === '=' || ['var', 'const', 'global', 'fn', 'for', 'as', 'catch'].includes(tokens[index - 1]?.text)) return undefined;
  const bindings = bindingsAt(tokens, token.start);
  let data;
  if (tokens[index - 1]?.text === '.') {
    const name = receiver(tokens, index - 1);
    const module = bindings.get(name)?.module;
    if (module && own(modules[module], token.text)) data = modules[module][token.text];
  } else {
    const binding = bindings.get(token.text);
    if (binding?.builtin) data = builtins[binding.builtin];
  }
  return data ? { ...data, start: token.start, end: token.end } : undefined;
}

module.exports = { scan, completions, hover };
