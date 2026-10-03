// Checks the stylesheet extract-css.mjs rebuilt against a shipped workbench.desktop.main.css.
//
// The shipped file is only read here, to measure the reconstruction; nothing from it is fed
// to blitz-dom. Both sheets are reduced to their sequence of top-level style rule selectors,
// one entry per selector in a selector list, normalised for what a minifier changes
// (whitespace, quotes, `::before` to `:before`, `100%` to `to`). The script reports how many
// selectors each sheet has and how many are shared, and then checks order at the level that
// decides the cascade between files: for each source file of the rebuilt sheet, the median
// shipped position of its selectors, and how many files keep their relative order (the
// longest increasing subsequence of those medians).
//
// usage: node compare-css.mjs <rebuilt.css> <shipped.css> [<out.json>]

import fs from 'node:fs';

function norm(s) {
	return s.replace(/\s+/g, '').replace(/["']/g, '').toLowerCase()
		.replace(/::(before|after)/g, ':$1').replace(/\b100%/g, 'to').replace(/(^|[|,])0%/g, '$1from');
}

function splitList(head) {
	const parts = [];
	let depth = 0;
	let cur = '';
	for (const c of head) {
		if (c === '(') depth++;
		if (c === ')') depth--;
		if (c === ',' && depth === 0) { parts.push(cur); cur = ''; } else cur += c;
	}
	parts.push(cur);
	return parts.map(x => x.trim()).filter(Boolean);
}

function selectors(css) {
	// The rebuilt sheet marks each source file with a /* vs/...css */ comment.
	css = css.replace(/\/\*\s*(vs\/[^\s*]+\.css)\s*\*\//g, (_, f) => '\u0001' + f + '\u0002');
	css = css.replace(/\/\*[\s\S]*?\*\//g, '');
	const out = [];
	let buf = '';
	let currentFile = null;
	const stack = [];
	for (let i = 0; i < css.length; i++) {
		const c = css[i];
		if (c === '\u0001') {
			const j = css.indexOf('\u0002', i);
			currentFile = css.slice(i + 1, j);
			i = j;
			buf = '';
			continue;
		}
		if (c === '{') {
			const head = buf.trim();
			if (!head.startsWith('@') && !stack.some(h => !h.startsWith('@'))) {
				const prefix = stack.join('|').replace(/\s+/g, '').toLowerCase();
				for (const one of splitList(head)) out.push({ sel: norm(prefix + '|' + one), file: currentFile });
			}
			stack.push(head);
			buf = '';
		} else if (c === '}') {
			stack.pop();
			buf = '';
		} else if (c === ';') {
			buf = '';
		} else {
			buf += c;
		}
	}
	return out;
}

function lisLength(a) {
	const tails = [];
	for (const x of a) {
		let lo = 0, hi = tails.length;
		while (lo < hi) { const mid = (lo + hi) >> 1; if (tails[mid] < x) lo = mid + 1; else hi = mid; }
		tails[lo] = x;
	}
	return tails.length;
}

const [, , rebuiltPath, shippedPath, outPath] = process.argv;
const rebuilt = selectors(fs.readFileSync(rebuiltPath, 'utf8'));
const shipped = selectors(fs.readFileSync(shippedPath, 'utf8'));

const shippedIndex = new Map();
shipped.forEach((s, i) => { if (!shippedIndex.has(s.sel)) shippedIndex.set(s.sel, []); shippedIndex.get(s.sel).push(i); });
const taken = new Map();
const onlyRebuilt = [];
const perFile = new Map();
let shared = 0;
for (const s of rebuilt) {
	const list = shippedIndex.get(s.sel);
	const k = taken.get(s.sel) || 0;
	if (list && k < list.length) {
		taken.set(s.sel, k + 1);
		shared++;
		if (list.length === 1) {
			if (!perFile.has(s.file)) perFile.set(s.file, []);
			perFile.get(s.file).push(list[0]);
		}
	} else {
		onlyRebuilt.push(s.sel);
	}
}
const rebuiltCount = new Map();
rebuilt.forEach(s => rebuiltCount.set(s.sel, (rebuiltCount.get(s.sel) || 0) + 1));
const seen = new Map();
const onlyShipped = [];
for (const s of shipped) {
	const k = (seen.get(s.sel) || 0) + 1;
	seen.set(s.sel, k);
	if (k > (rebuiltCount.get(s.sel) || 0)) onlyShipped.push(s.sel);
}

const files = [...new Set(rebuilt.map(s => s.file))];
const medians = [];
const outOfOrder = [];
for (const f of files) {
	const ps = (perFile.get(f) || []).slice().sort((a, b) => a - b);
	if (ps.length) medians.push({ f, m: ps[ps.length >> 1] });
}
const inOrder = lisLength(medians.map(x => x.m));
for (let i = 1; i < medians.length; i++) if (medians[i].m < medians[i - 1].m) outOfOrder.push(`${medians[i - 1].f} -> ${medians[i].f}`);

const result = {
	rebuiltSelectors: rebuilt.length,
	shippedSelectors: shipped.length,
	sharedSelectors: shared,
	onlyInRebuilt: onlyRebuilt.length,
	onlyInShipped: onlyShipped.length,
	cssFilesRebuilt: files.length,
	cssFilesLocated: medians.length,
	cssFilesInShippedOrder: inOrder,
	adjacentInversions: outOfOrder.length,
	sampleInversions: outOfOrder.slice(0, 10),
	sampleOnlyInRebuilt: onlyRebuilt.slice(0, 10),
	sampleOnlyInShipped: onlyShipped.slice(0, 10),
};
console.log(JSON.stringify(result, null, 1));
if (outPath) fs.writeFileSync(outPath, JSON.stringify(result, null, 1));
