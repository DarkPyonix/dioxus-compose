// Lays the static fixtures out in VS Code's own Chromium and records the same geometry
// capture-vscode.mjs recorded from the live workbench. If the two agree, the fixtures are a
// faithful reproduction and any difference blitz-dom shows is blitz-dom's.
//
// This replaces the document of the running VS Code window (document.open/write), so run it
// after capture-vscode.mjs and quit VS Code afterwards. The rebuilt stylesheet and the
// runtime stylesheet are inlined as <style> elements, because the window's vscode-file:
// protocol only serves files from inside the application. The codicon @font-face is pointed
// at the copy of codicon.ttf inside the application for the same reason.
//
// usage: node check-fixtures.mjs <port> <experiment dir> <rebuilt workbench.css>
// writes: captured/<region>.chrome-fixture.json, captured/text.chrome-fixture.json

import fs from 'node:fs';
import path from 'node:path';

const [, , port, expDir, cssPath] = process.argv;
const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = targets.find(t => t.type === 'page' && /workbench/.test(t.url));
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

// The workbench page requires Trusted Types for document.write. Bypassing its content
// security policy only takes effect on the next load, so reload once first.
await send('Page.enable');
await send('Page.setBypassCSP', { enabled: true });
await send('Page.reload', { ignoreCache: false });
await new Promise(r => setTimeout(r, 8000));

const appCodicon = 'vscode-file://vscode-app/Applications/Visual%20Studio%20Code.app/Contents/Resources/app/out/media/codicon.ttf';
const workbenchCss = fs.readFileSync(cssPath, 'utf8').replace('url("./codicon.ttf")', `url("${appCodicon}")`);
const runtimeCss = fs.readFileSync(path.join(expDir, 'captured/runtime.css'), 'utf8');

function inline(html) {
	return html
		.replace(/<link rel="stylesheet" href="[^"]*workbench\.css">/, () => `<style>${workbenchCss}</style>`)
		.replace(/<link rel="stylesheet" href="[^"]*runtime\.css">/, () => `<style>${runtimeCss}</style>`);
}

const measureRegion = `(async () => {
	await document.fonts.ready;
	await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)));
	const elements = [];
	const texts = [];
	for (const el of document.querySelectorAll('[data-probe-id]')) {
		const id = Number(el.getAttribute('data-probe-id'));
		const r = el.getBoundingClientRect();
		elements.push({ id, x: r.left, y: r.top, w: r.width, h: r.height });
		for (const node of el.childNodes) {
			if (node.nodeType !== 3 || !node.data.trim()) continue;
			const range = document.createRange();
			range.selectNodeContents(node);
			texts.push({ parent: id, text: node.data, rects: [...range.getClientRects()].map(q => ({ x: q.left, y: q.top, w: q.width, h: q.height })) });
		}
	}
	return { elements, texts, viewport: { width: innerWidth, height: innerHeight, dpr: devicePixelRatio } };
})()`;

const measureText = `(async () => {
	await document.fonts.ready;
	const results = [];
	for (const box of document.querySelectorAll('[data-case]')) {
		const node = box.firstChild;
		const b = box.getBoundingClientRect();
		const range = document.createRange();
		const chars = [];
		let offset = 0;
		for (const ch of node.data) {
			range.setStart(node, offset);
			range.setEnd(node, offset + ch.length);
			const r = [...range.getClientRects()][0];
			chars.push(r ? { x: r.left - b.left, y: r.top - b.top, w: r.width, h: r.height } : null);
			offset += ch.length;
		}
		range.selectNodeContents(node);
		const lines = [...range.getClientRects()].map(q => ({ x: q.left - b.left, y: q.top - b.top, w: q.width, h: q.height }));
		results.push({ id: box.getAttribute('data-case'), box: { w: b.width, h: b.height }, lines, chars });
	}
	return { results };
})()`;

for (const name of ['activitybar', 'sidebar', 'tabs', 'text']) {
	const html = inline(fs.readFileSync(path.join(expDir, 'fixtures', `${name}.html`), 'utf8'));
	await evaluate(`(() => { document.open(); document.write(${JSON.stringify(html)}); document.close(); return true; })()`);
	const result = await evaluate(name === 'text' ? measureText : measureRegion);
	fs.writeFileSync(path.join(expDir, 'captured', `${name}.chrome-fixture.json`), JSON.stringify(result, null, 1));
	console.log(name, name === 'text' ? result.results.length : result.elements.length);
}
ws.close();
