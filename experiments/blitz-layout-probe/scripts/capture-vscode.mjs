// Reads the workbench DOM and its geometry out of a running VS Code over the Chrome DevTools
// Protocol, and writes the fixtures the blitz-dom probe lays out.
//
// VS Code must already be running with --remote-debugging-port=<port> (see README.md for the
// exact command, which uses a fresh --user-data-dir so nothing of the user's is touched).
//
// For each region it writes:
//   fixtures/<region>.html     the region's subtree, wrapped in its real ancestor chain (each
//                              ancestor with its own attributes and inline style but without
//                              siblings), plus every stylesheet the page built at run time
//                              (theme colours as CSS variables, icon fonts, layout rules
//                              computed in script). Every element carries data-probe-id.
//   captured/<region>.json     every element's getBoundingClientRect and a few computed
//                              properties, and every text node's client rects, keyed by
//                              data-probe-id.
// And for text:
//   fixtures/text.html, captured/text.json
//                              the strings in fixtures/text-cases.json laid out in the
//                              workbench font, with each character's rect, so line breaks
//                              and line widths can be compared.
//
// usage: node capture-vscode.mjs <port> <experiment dir> <rebuilt css path relative to fixtures>

import fs from 'node:fs';
import path from 'node:path';

const [, , port, expDir, cssHref] = process.argv;
if (!port || !expDir || !cssHref) {
	console.error('usage: node capture-vscode.mjs <port> <experiment dir> <css href>');
	process.exit(2);
}

const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = targets.find(t => t.type === 'page' && /workbench/.test(t.url));
if (!page) throw new Error(`no workbench page among ${targets.map(t => t.url).join(', ')}`);

const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((ok, err) => { ws.onopen = ok; ws.onerror = err; });
let nextId = 1;
const pending = new Map();
ws.onmessage = ev => {
	const msg = JSON.parse(ev.data);
	if (msg.id && pending.has(msg.id)) {
		const { ok, err } = pending.get(msg.id);
		pending.delete(msg.id);
		if (msg.error) err(new Error(JSON.stringify(msg.error))); else ok(msg.result);
	}
};
function send(method, params = {}) {
	const id = nextId++;
	ws.send(JSON.stringify({ id, method, params }));
	return new Promise((ok, err) => pending.set(id, { ok, err }));
}
async function evaluate(expr) {
	const r = await send('Runtime.evaluate', { expression: expr, returnByValue: true, awaitPromise: true });
	if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails));
	return r.result.value;
}
const sleep = ms => new Promise(r => setTimeout(r, ms));

// Wait for the explorer and the tabs to settle.
for (let i = 0; i < 60; i++) {
	const ready = await evaluate(`({
		rows: document.querySelectorAll('.explorer-folders-view .monaco-list-row').length,
		tabs: document.querySelectorAll('.tabs-container > .tab').length,
	})`);
	if (ready.rows >= 4 && ready.tabs >= 4) break;
	await sleep(500);
}

// Expand every collapsed folder in the explorer with real mouse clicks, so the tree has
// nesting at more than one level.
for (let pass = 0; pass < 6; pass++) {
	const target = await evaluate(`(() => {
		const row = [...document.querySelectorAll('.explorer-folders-view .monaco-list-row[aria-expanded="false"]')][0];
		if (!row) return null;
		const r = row.getBoundingClientRect();
		return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
	})()`);
	if (!target) break;
	for (const type of ['mousePressed', 'mouseReleased']) {
		await send('Input.dispatchMouseEvent', { type, x: target.x, y: target.y, button: 'left', clickCount: 1 });
	}
	await sleep(400);
}
// Move the pointer off every region so no hover state is captured.
await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 600, y: 500 });
await sleep(800);

const regions = {
	activitybar: '.part.activitybar',
	sidebar: '.part.sidebar',
	tabs: '.part.editor .title.tabs',
};

const collect = `(async (regions, cssHref) => {
	const esc = s => s.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;');
	const openTag = el => '<' + el.localName + [...el.attributes].map(a => ' ' + a.name + '="' + esc(a.value) + '"').join('') + '>';
	const runtimeCss = [];
	for (const sheet of document.styleSheets) {
		if (sheet.href) continue;
		let text = '';
		try { text = [...sheet.cssRules].map(r => r.cssText).join('\\n'); } catch (e) { text = '/* unreadable */'; }
		runtimeCss.push(text);
	}
	const hrefs = [...document.styleSheets].map(s => s.href).filter(Boolean);
	const props = ['display', 'position', 'box-sizing', 'font-family', 'font-size', 'font-weight', 'line-height', 'letter-spacing', 'white-space', 'overflow', 'flex-direction', 'align-items', 'justify-content', 'width', 'height', 'padding-left', 'padding-top', 'margin-left', 'margin-top', 'text-overflow', 'visibility'];
	const out = {
		viewport: { width: innerWidth, height: innerHeight, dpr: devicePixelRatio },
		htmlAttrs: Object.fromEntries([...document.documentElement.attributes].map(a => [a.name, a.value])),
		linkedSheets: hrefs,
		regions: {},
	};
	for (const [name, sel] of Object.entries(regions)) {
		const root = document.querySelector(sel);
		if (!root) { out.regions[name] = { error: 'not found: ' + sel }; continue; }
		const clone = root.cloneNode(true);
		const live = [root, ...root.querySelectorAll('*')];
		const copy = [clone, ...clone.querySelectorAll('*')];
		const elements = [];
		const texts = [];
		live.forEach((el, i) => {
			copy[i].setAttribute('data-probe-id', String(i));
			const r = el.getBoundingClientRect();
			const cs = getComputedStyle(el);
			elements.push({
				id: i, tag: el.localName, cls: el.getAttribute('class') || '',
				x: r.left, y: r.top, w: r.width, h: r.height,
				style: Object.fromEntries(props.map(p => [p, cs.getPropertyValue(p)])),
				before: getComputedStyle(el, '::before').content,
			});
			for (const node of el.childNodes) {
				if (node.nodeType !== 3 || !node.data.trim()) continue;
				const range = document.createRange();
				range.selectNodeContents(node);
				const rects = [...range.getClientRects()].map(q => ({ x: q.left, y: q.top, w: q.width, h: q.height }));
				texts.push({ parent: i, text: node.data, rects });
			}
		});
		let wrapped = clone.outerHTML;
		let closing = '';
		for (let a = root.parentElement; a && a !== document.documentElement; a = a.parentElement) {
			wrapped = openTag(a) + wrapped;
			closing = closing + '</' + a.localName + '>';
		}
		out.regions[name] = { selector: sel, html: wrapped + closing, elements, texts };
	}
	return out;
})(${JSON.stringify(regions)}, ${JSON.stringify(cssHref)})`;

const data = await evaluate(collect);

// Which font the page actually drew the workbench text with, from the browser itself.
await send('DOM.enable');
await send('CSS.enable');
const doc = await send('DOM.getDocument', { depth: -1 });
const fonts = {};
for (const [key, sel] of Object.entries({
	tabLabel: '.tabs-container > .tab .label-name',
	explorerRow: '.explorer-folders-view .monaco-list-row .label-name',
	sidebarTitle: '.part.sidebar .title-label h2',
})) {
	const q = await send('DOM.querySelector', { nodeId: doc.root.nodeId, selector: sel });
	if (!q.nodeId) { fonts[key] = 'not found'; continue; }
	const f = await send('CSS.getPlatformFontsForNode', { nodeId: q.nodeId });
	fonts[key] = f.fonts;
}
data.platformFonts = fonts;

const fixtures = path.join(expDir, 'fixtures');
const captured = path.join(expDir, 'captured');
fs.mkdirSync(fixtures, { recursive: true });
fs.mkdirSync(captured, { recursive: true });

const htmlAttrs = Object.entries(data.htmlAttrs).map(([k, v]) => ` ${k}="${v.replace(/"/g, '&quot;')}"`).join('');
const runtimeStyle = `<style data-probe="runtime">\n${(await evaluate(`[...document.styleSheets].filter(s => !s.href).map(s => { try { return [...s.cssRules].map(r => r.cssText).join('\\n'); } catch (e) { return ''; } }).join('\\n')`))}\n</style>`;
fs.writeFileSync(path.join(captured, 'runtime.css'), runtimeStyle.replace(/^<style[^>]*>\n|\n<\/style>$/g, ''));

function htmlPage(body) {
	return `<!DOCTYPE html>\n<html${htmlAttrs}>\n<head>\n<meta charset="utf-8">\n<link rel="stylesheet" href="${cssHref}">\n<link rel="stylesheet" href="../captured/runtime.css">\n</head>\n${body}\n</html>\n`;
}

for (const [name, region] of Object.entries(data.regions)) {
	if (region.error) { console.error(name, region.error); continue; }
	// The ancestor chain starts with <body>; keep it as the page's body.
	fs.writeFileSync(path.join(fixtures, `${name}.html`), htmlPage(region.html));
	fs.writeFileSync(path.join(captured, `${name}.json`), JSON.stringify({
		viewport: data.viewport, selector: region.selector, elements: region.elements, texts: region.texts,
	}, null, 1));
	console.log(`${name}: ${region.elements.length} elements, ${region.texts.length} text nodes`);
}

// Text: lay each case out inside the workbench (so it inherits the workbench font) in a
// box of the case's width, and record every character's rect.
const cases = JSON.parse(fs.readFileSync(path.join(fixtures, 'text-cases.json'), 'utf8'));
const textData = await evaluate(`((cases) => {
	const host = document.createElement('div');
	host.id = 'probe-text-host';
	host.style.cssText = 'position:fixed;left:0;top:0;z-index:100000;background:#000;';
	document.querySelector('.monaco-workbench').appendChild(host);
	const results = [];
	for (const c of cases) {
		const box = document.createElement('div');
		box.setAttribute('data-case', c.id);
		box.style.cssText = 'font-size:' + c.fontSize + 'px;font-weight:' + (c.fontWeight || 400) + ';line-height:' + (c.lineHeight || 'normal') + ';' + (c.width ? 'width:' + c.width + 'px;white-space:normal;' : 'width:max-content;white-space:nowrap;');
		box.textContent = c.text;
		host.appendChild(box);
		const node = box.firstChild;
		const b = box.getBoundingClientRect();
		const chars = [];
		const range = document.createRange();
		// Walk by code point so surrogate pairs stay whole.
		let offset = 0;
		for (const ch of c.text) {
			range.setStart(node, offset);
			range.setEnd(node, offset + ch.length);
			const r = [...range.getClientRects()][0];
			chars.push(r ? { x: r.left - b.left, y: r.top - b.top, w: r.width, h: r.height } : null);
			offset += ch.length;
		}
		range.selectNodeContents(node);
		const lines = [...range.getClientRects()].map(q => ({ x: q.left - b.left, y: q.top - b.top, w: q.width, h: q.height }));
		results.push({ id: c.id, box: { w: b.width, h: b.height }, lines, chars, font: getComputedStyle(box).fontFamily });
	}
	const html = host.outerHTML;
	host.remove();
	return { results, html };
})(${JSON.stringify(cases)})`);

// The text fixture: the same boxes, inside the same workbench element and classes.
const wb = await evaluate(`(() => { const w = document.querySelector('.monaco-workbench'); return '<' + w.localName + [...w.attributes].filter(a => a.name !== 'style').map(a => ' ' + a.name + '="' + a.value.replace(/"/g, '&quot;') + '"').join('') + '>'; })()`);
fs.writeFileSync(path.join(fixtures, 'text.html'), htmlPage(`<body>\n${wb}${textData.html}</div>\n</body>`));
fs.writeFileSync(path.join(captured, 'text.json'), JSON.stringify({ viewport: data.viewport, results: textData.results }, null, 1));
fs.writeFileSync(path.join(captured, 'meta.json'), JSON.stringify({
	viewport: data.viewport, htmlAttrs: data.htmlAttrs, linkedSheets: data.linkedSheets, platformFonts: data.platformFonts,
	userAgent: await evaluate('navigator.userAgent'),
}, null, 1));
console.log('text cases:', textData.results.length);
console.log('platform fonts:', JSON.stringify(data.platformFonts));
ws.close();
