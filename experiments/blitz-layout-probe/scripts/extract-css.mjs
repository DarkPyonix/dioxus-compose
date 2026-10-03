// Rebuilds the stylesheet the VS Code workbench loads (workbench.desktop.main.css) from a
// Code-OSS source checkout, without npm, node_modules or esbuild.
//
// The real build (build/next/index.ts in Code-OSS) runs esbuild over
// src/vs/workbench/workbench.desktop.main.ts with bundle: true, packages: 'external' and no
// code splitting, and lets esbuild concatenate every CSS file the module graph imports.
// esbuild orders CSS imported from JS by JS evaluation order: a depth-first walk over each
// module's import records in source order, where a module is visited once, at its first
// import. This script does the same walk over the TypeScript sources.
//
// esbuild drops a TypeScript import whose bindings are only used as types. This script
// cannot type-check, so it approximates: `import type` and imports whose every specifier is
// `type` are dropped, and any other import is kept if one of its bindings appears anywhere
// in the rest of the file. That can keep an import esbuild would drop, which can move a CSS
// file earlier. compare-css.mjs measures how far the result is from a shipped build.
//
// usage: node extract-css.mjs <code-oss checkout> <out.css> [<out-order.json>]

import fs from 'node:fs';
import path from 'node:path';

const [, , root, outCss, outOrder] = process.argv;
if (!root || !outCss) {
	console.error('usage: node extract-css.mjs <code-oss checkout> <out.css> [<out-order.json>]');
	process.exit(2);
}
const src = path.join(root, 'src');
const entry = path.join(src, 'vs/workbench/workbench.desktop.main.ts');

function stripComments(text) {
	// Removes // and /* */ comments while leaving string and template literals intact.
	let out = '';
	let i = 0;
	const n = text.length;
	while (i < n) {
		const c = text[i];
		const d = text[i + 1];
		if (c === '/' && d === '/') {
			while (i < n && text[i] !== '\n') i++;
		} else if (c === '/' && d === '*') {
			i += 2;
			while (i < n && !(text[i] === '*' && text[i + 1] === '/')) i++;
			i += 2;
		} else if (c === '\'' || c === '"' || c === '`') {
			const q = c;
			out += c;
			i++;
			while (i < n && text[i] !== q) {
				if (text[i] === '\\') { out += text[i]; i++; }
				out += text[i];
				i++;
			}
			out += q;
			i++;
		} else {
			out += c;
			i++;
		}
	}
	return out;
}

const importRe = /(?:^|[;\n}])\s*(import|export)\s+([^;'"`]*?)\s*from\s*['"]([^'"]+)['"]|(?:^|[;\n}])\s*import\s*['"]([^'"]+)['"]|\bimport\s*\(\s*['"]([^'"]+)['"]\s*\)/g;

function bindingsOf(clause) {
	// clause: `X`, `{ a, b as c, type d }`, `* as ns`, `X, { a }`, `type { ... }`
	const names = [];
	let allType = true;
	const braced = clause.match(/\{([\s\S]*)\}/);
	const rest = clause.replace(/\{[\s\S]*\}/, '').split(',').map(s => s.trim()).filter(Boolean);
	for (const r of rest) {
		const ns = r.match(/^\*\s+as\s+(\w+)/);
		if (ns) { names.push(ns[1]); allType = false; } else if (/^\w+$/.test(r)) { names.push(r); allType = false; }
	}
	if (braced) {
		for (const spec of braced[1].split(',').map(s => s.trim()).filter(Boolean)) {
			const isType = /^type\s+/.test(spec);
			const s = spec.replace(/^type\s+/, '');
			const local = (s.match(/\bas\s+(\w+)$/) || [null, s.match(/^(\w+)/)?.[1]])[1];
			if (!isType) { allType = false; if (local) names.push(local); }
		}
	}
	return { names, allType };
}

function importsOf(file) {
	const text = stripComments(fs.readFileSync(file, 'utf8'));
	const records = [];
	let m;
	importRe.lastIndex = 0;
	const matches = [];
	while ((m = importRe.exec(text))) matches.push({ m, index: m.index, end: importRe.lastIndex });
	// Text with the import statements removed, to test whether a binding is used.
	let body = '';
	let last = 0;
	for (const { m, index, end } of matches) {
		if (m[1] === 'import') { body += text.slice(last, index); last = end; }
	}
	body += text.slice(last);
	for (const { m } of matches) {
		if (m[4] || m[5]) { records.push(m[4] || m[5]); continue; }
		const kind = m[1];
		let clause = m[2];
		if (/^type\b/.test(clause)) continue;
		if (kind === 'export') { records.push(m[3]); continue; }
		const { names, allType } = bindingsOf(clause);
		if (allType && names.length === 0 && /\{/.test(clause)) continue;
		if (names.some(n => usedAsValue(body, n))) records.push(m[3]);
	}
	return records;
}

// Whether `name` is used in a value position somewhere in `body`. An occurrence counts as a
// type when the token before it is one that only introduces a type (`:`, `<`, `|`, `&`,
// `as`, `implements`, `satisfies`, `keyof`, `is`, `extends` inside an interface or a type
// parameter list) or when it is followed by `[]` or `>`. Everything else is a value.
function usedAsValue(body, name) {
	const re = new RegExp(`(^|[^\\w$.])${name.replace(/\$/g, '\\$')}(?![\\w$])`, 'g');
	let m;
	while ((m = re.exec(body))) {
		const at = m.index + m[1].length;
		const before = body.slice(Math.max(0, at - 200), at);
		const after = body.slice(at + name.length, at + name.length + 40);
		const last = (before.match(/(\S*)\s*$/) || ['', ''])[1];
		const punct = last.match(/[:<|&,(]$/);
		const tok = punct ? punct[0] : (last.match(/\w+$/) || [last])[0];
		// A call or a lower-case member access is a value whatever precedes it (an object
		// literal property `options: f(x)` also follows a `:`).
		if (/^\s*(\(|\.\s*[a-z_$])/.test(after)) return true;
		// The `:` of a conditional expression `c ? a : b` is not a type annotation.
		if (tok === ':') {
			const q = before.search(/\s\?\s[^;{}?]*:\s*$/);
			if (q >= 0) return true;
		}
		if ([':', '<', '|', '&', 'as', 'implements', 'satisfies', 'keyof', 'is', 'readonly'].includes(tok)) continue;
		if (tok === 'extends') {
			const line = before.slice(before.lastIndexOf('\n') + 1);
			if (/\binterface\b|<[^>]*$|\btype\b/.test(line)) continue;
			return true;
		}
		if (/^\s*(\[\]|>)/.test(after)) continue;
		if (/^\s*\.\s*I[A-Z]\w*\s*[\s,>\[;=)|&]/.test(after) && tok !== '(' && tok !== '=') continue;
		if (tok === ',') {
			// `Map<string, X>` against `f(a, X)`: inside an open `<` with no `(` after it.
			const lt = before.lastIndexOf('<');
			const paren = before.lastIndexOf('(');
			if (lt > paren && !before.slice(lt).includes('>')) continue;
		}
		return true;
	}
	return false;
}

function resolve(from, spec) {
	if (!spec.startsWith('.')) return null; // package, external in the real build
	let p = path.resolve(path.dirname(from), spec);
	if (p.endsWith('.css')) return fs.existsSync(p) ? p : null;
	if (p.endsWith('.js')) p = p.slice(0, -3);
	for (const cand of [p + '.ts', p + '.tsx', path.join(p, 'index.ts')]) {
		if (fs.existsSync(cand)) return cand;
	}
	return null;
}

const visited = new Set();
const cssOrder = [];
const cssSeen = new Set();
function visit(file) {
	visited.add(file);
	for (const spec of importsOf(file)) {
		const r = resolve(file, spec);
		if (!r) continue;
		if (r.endsWith('.css')) {
			if (!cssSeen.has(r)) { cssSeen.add(r); cssOrder.push(r); }
		} else if (!visited.has(r)) {
			visit(r);
		}
	}
}
visit(entry);

let css = '';
for (const f of cssOrder) {
	css += `/* ${path.relative(src, f)} */\n` + fs.readFileSync(f, 'utf8') + '\n';
}
fs.writeFileSync(outCss, css);
if (outOrder) fs.writeFileSync(outOrder, JSON.stringify(cssOrder.map(f => path.relative(src, f)), null, 1));
console.log(`modules visited: ${visited.size}, css files: ${cssOrder.length}, bytes: ${css.length}`);
