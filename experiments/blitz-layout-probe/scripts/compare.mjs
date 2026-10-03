// Compares a candidate layout of the fixtures against what the live VS Code workbench drew.
//
// The reference is always captured/<region>.json and captured/text.json, read out of the
// running VS Code by capture-vscode.mjs. The candidate is a file per region of the same shape:
//   { elements: [{ id, x, y, w, h }], texts: [{ parent, text, rects: [{ x, y, w, h }] }] }
// and for text { results: [{ id, box: { w, h }, lines: [...], chars: [{ x, y, w, h }] }] }.
// check-fixtures.mjs writes one (`chrome-fixture`, the fixtures laid out by Chromium) and
// the Rust probe writes another (`blitz`).
//
// usage: node compare.mjs <experiment dir> <candidate suffix> [<out.json>]
//   e.g. node compare.mjs . blitz results-blitz.json

import fs from 'node:fs';
import path from 'node:path';

const [, , expDir, suffix, outPath] = process.argv;
if (!expDir || !suffix) {
	console.error('usage: node compare.mjs <experiment dir> <candidate suffix> [<out.json>]');
	process.exit(2);
}
const cap = f => JSON.parse(fs.readFileSync(path.join(expDir, 'captured', f), 'utf8'));

const SKIP_TAGS = new Set(['style', 'script', 'template', 'svg', 'path']);

function quantiles(xs) {
	if (!xs.length) return null;
	const s = xs.slice().sort((a, b) => a - b);
	const q = p => s[Math.min(s.length - 1, Math.floor(p * (s.length - 1) + 0.5))];
	const round = v => Math.round(v * 100) / 100;
	return { n: s.length, median: round(q(0.5)), p90: round(q(0.9)), p99: round(q(0.99)), max: round(s[s.length - 1]), mean: round(s.reduce((a, b) => a + b, 0) / s.length) };
}

function within(xs, ts) {
	return Object.fromEntries(ts.map(t => [`<=${t}px`, xs.filter(x => x <= t).length]));
}

function describe(el) {
	const cls = el.cls ? '.' + el.cls.trim().split(/\s+/).slice(0, 3).join('.') : '';
	return `${el.tag}${cls}#${el.id}`;
}

function compareRegion(name) {
	const live = cap(`${name}.json`);
	const candFile = path.join(expDir, 'captured', `${name}.${suffix}.json`);
	if (!fs.existsSync(candFile)) return { error: `missing ${candFile}` };
	const cand = JSON.parse(fs.readFileSync(candFile, 'utf8'));
	const byId = new Map(cand.elements.map(e => [e.id, e]));

	// Elements inside a skipped element (the contents of an <svg>, say) are skipped too.
	const skipped = new Set();
	const parentOf = new Map();
	for (const el of live.elements) {
		if (SKIP_TAGS.has(el.tag)) skipped.add(el.id);
	}

	// Elements the static fixture itself does not reproduce (when Chromium lays the fixture
	// out, they land somewhere other than in the live window) are excluded, because the
	// difference is in the fixture, not in the engine under test. In practice these are the
	// explorer's header actions, which the live window showed because the tree had focus.
	const excludedByFixture = new Set();
	const fixtureFile = path.join(expDir, 'captured', `${name}.chrome-fixture.json`);
	if (suffix !== 'chrome-fixture' && fs.existsSync(fixtureFile)) {
		const fx = new Map(JSON.parse(fs.readFileSync(fixtureFile, 'utf8')).elements.map(e => [e.id, e]));
		for (const el of live.elements) {
			const f = fx.get(el.id);
			if (f && Math.max(Math.abs(f.x - el.x), Math.abs(f.y - el.y), Math.abs(f.w - el.w), Math.abs(f.h - el.h)) > 0.5) excludedByFixture.add(el.id);
		}
	}

	const rows = [];
	let missing = 0;
	let hiddenBoth = 0;
	for (const el of live.elements) {
		if (skipped.has(el.id) || excludedByFixture.has(el.id)) continue;
		const c = byId.get(el.id);
		if (!c) { missing++; continue; }
		const liveHidden = el.w === 0 && el.h === 0;
		const candHidden = c.w === 0 && c.h === 0;
		if (liveHidden && candHidden) { hiddenBoth++; continue; }
		const dx = c.x - el.x, dy = c.y - el.y, dw = c.w - el.w, dh = c.h - el.h;
		rows.push({
			el: describe(el), id: el.id, display: el.style?.display,
			live: [el.x, el.y, el.w, el.h].map(v => Math.round(v * 100) / 100),
			cand: [c.x, c.y, c.w, c.h].map(v => Math.round(v * 100) / 100),
			dx, dy, dw, dh,
			err: Math.max(Math.abs(dx), Math.abs(dy), Math.abs(dw), Math.abs(dh)),
			pos: Math.max(Math.abs(dx), Math.abs(dy)),
			size: Math.max(Math.abs(dw), Math.abs(dh)),
			visibility: liveHidden ? 'hidden in VS Code only' : candHidden ? 'hidden in candidate only' : 'both visible',
		});
	}
	const errs = rows.map(r => r.err);
	const worst = rows.slice().sort((a, b) => b.err - a.err).slice(0, 15).map(r => ({
		el: r.el, display: r.display, live: r.live, cand: r.cand, dx: +r.dx.toFixed(2), dy: +r.dy.toFixed(2), dw: +r.dw.toFixed(2), dh: +r.dh.toFixed(2), visibility: r.visibility,
	}));

	// Text nodes, matched by parent id and order within the parent.
	const key = t => `${t.parent}:${t.text}`;
	const candTexts = new Map();
	for (const t of cand.texts || []) candTexts.set(key(t), t);
	const textRows = [];
	for (const t of live.texts) {
		if (skipped.has(t.parent)) continue;
		const parentEl = live.elements.find(e => e.id === t.parent);
		if (parentEl && SKIP_TAGS.has(parentEl.tag)) continue;
		const c = candTexts.get(key(t));
		if (!t.rects.length) continue;
		if (!c || !c.rects.length) { textRows.push({ text: t.text, missing: true }); continue; }
		const a = t.rects[0], b = c.rects[0];
		const wl = t.rects.reduce((s, r) => s + r.w, 0);
		const wc = c.rects.reduce((s, r) => s + r.w, 0);
		textRows.push({
			text: t.text, lines: [t.rects.length, c.rects.length],
			dx: +(b.x - a.x).toFixed(2), dy: +(b.y - a.y).toFixed(2), dw: +(wc - wl).toFixed(2), dh: +(b.h - a.h).toFixed(2),
			widthLive: +wl.toFixed(2), widthCand: +wc.toFixed(2),
		});
	}
	const present = textRows.filter(r => !r.missing);
	return {
		elementsCompared: rows.length,
		elementsExcludedBecauseTheFixtureDiffers: excludedByFixture.size,
		elementsHiddenInBoth: hiddenBoth,
		elementsMissingFromCandidate: missing,
		hiddenInVsCodeOnly: rows.filter(r => r.visibility === 'hidden in VS Code only').length,
		hiddenInCandidateOnly: rows.filter(r => r.visibility === 'hidden in candidate only').length,
		maxAbsError: quantiles(errs),
		positionError: quantiles(rows.map(r => r.pos)),
		sizeError: quantiles(rows.map(r => r.size)),
		elementsWithin: within(errs, [0.5, 1, 2, 5, 10]),
		worst,
		texts: {
			compared: present.length,
			missing: textRows.length - present.length,
			lineCountMismatches: present.filter(r => r.lines[0] !== r.lines[1]).length,
			widthError: quantiles(present.map(r => Math.abs(r.dw))),
			positionError: quantiles(present.map(r => Math.max(Math.abs(r.dx), Math.abs(r.dy)))),
			rows: textRows,
		},
	};
}

function lineBreaks(chars) {
	// Indices of the characters that start a new line, from the change in their top edge.
	const breaks = [];
	let lastY = null;
	chars.forEach((c, i) => {
		if (!c) return;
		if (lastY !== null && c.y > lastY + 1) breaks.push(i);
		lastY = c.y;
	});
	return breaks;
}

function compareText() {
	const live = cap('text.json');
	const candFile = path.join(expDir, 'captured', `text.${suffix}.json`);
	if (!fs.existsSync(candFile)) return { error: `missing ${candFile}` };
	const cand = JSON.parse(fs.readFileSync(candFile, 'utf8'));
	const byId = new Map(cand.results.map(r => [r.id, r]));
	const cases = JSON.parse(fs.readFileSync(path.join(expDir, 'fixtures', 'text-cases.json'), 'utf8'));
	const rows = [];
	for (const l of live.results) {
		const c = byId.get(l.id);
		const spec = cases.find(x => x.id === l.id);
		if (!c) { rows.push({ id: l.id, missing: true }); continue; }
		const lb = lineBreaks(l.chars), cb = lineBreaks(c.chars);
		// Natural width of the text: the right edge of the last character on the widest line.
		const lineWidths = rs => rs.map(r => r.w);
		const lw = lineWidths(l.lines), cw = lineWidths(c.lines);
		let charErr = 0;
		const n = Math.min(l.chars.length, c.chars.length);
		const sameBreaks = JSON.stringify(lb) === JSON.stringify(cb);
		for (let i = 0; i < n; i++) {
			if (!l.chars[i] || !c.chars[i]) continue;
			// Only meaningful while both are on the same line structure.
			if (!sameBreaks) break;
			charErr = Math.max(charErr, Math.abs(l.chars[i].x - c.chars[i].x));
		}
		const widest = xs => xs.length ? Math.max(...xs) : 0;
		rows.push({
			id: l.id, fontSize: spec?.fontSize, fontWeight: spec?.fontWeight || 400, width: spec?.width || null,
			lines: [l.lines.length, c.lines.length],
			breaksLive: lb, breaksCand: cb, sameBreaks,
			widthLive: +widest(lw).toFixed(3), widthCand: +widest(cw).toFixed(3),
			widthDiff: +(widest(cw) - widest(lw)).toFixed(3),
			widthDiffPct: widest(lw) ? +(100 * (widest(cw) - widest(lw)) / widest(lw)).toFixed(2) : null,
			heightLive: +l.box.h.toFixed(2), heightCand: +c.box.h.toFixed(2),
			maxCharXDiff: sameBreaks ? +charErr.toFixed(3) : null,
		});
	}
	const ok = rows.filter(r => !r.missing);
	const single = ok.filter(r => !r.width);
	const wrapped = ok.filter(r => r.width);
	return {
		cases: rows.length,
		sameLineBreaks: `${ok.filter(r => r.sameBreaks).length}/${ok.length}`,
		wrappedCasesWithSameBreaks: `${wrapped.filter(r => r.sameBreaks).length}/${wrapped.length}`,
		singleLineWidthDiff: quantiles(single.map(r => Math.abs(r.widthDiff))),
		singleLineWidthDiffPct: quantiles(single.map(r => Math.abs(r.widthDiffPct))),
		maxCharXDiff: quantiles(ok.filter(r => r.maxCharXDiff !== null).map(r => r.maxCharXDiff)),
		heightDiff: quantiles(ok.map(r => Math.abs(r.heightCand - r.heightLive))),
		rows,
	};
}

const result = {
	candidate: suffix,
	regions: Object.fromEntries(['activitybar', 'sidebar', 'tabs'].map(r => [r, compareRegion(r)])),
	text: compareText(),
};
const brief = {
	candidate: suffix,
	regions: Object.fromEntries(Object.entries(result.regions).map(([k, v]) => [k, v.error ? v : {
		compared: v.elementsCompared, excluded: v.elementsExcludedBecauseTheFixtureDiffers, missing: v.elementsMissingFromCandidate, maxAbsError: v.maxAbsError, within: v.elementsWithin,
		hiddenInVsCodeOnly: v.hiddenInVsCodeOnly, hiddenInCandidateOnly: v.hiddenInCandidateOnly,
		texts: { compared: v.texts.compared, missing: v.texts.missing, lineCountMismatches: v.texts.lineCountMismatches, widthError: v.texts.widthError, positionError: v.texts.positionError },
	}])),
	text: result.text.error ? result.text : {
		sameLineBreaks: result.text.sameLineBreaks, wrappedCasesWithSameBreaks: result.text.wrappedCasesWithSameBreaks,
		singleLineWidthDiff: result.text.singleLineWidthDiff, singleLineWidthDiffPct: result.text.singleLineWidthDiffPct,
		maxCharXDiff: result.text.maxCharXDiff, heightDiff: result.text.heightDiff,
	},
};
console.log(JSON.stringify(brief, null, 1));
if (outPath) fs.writeFileSync(outPath, JSON.stringify(result, null, 1));
