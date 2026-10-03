// Collects the probe's numbers into results.json.
//
// Everything that does not need blitz-dom (the stylesheet reconstruction check, the fixture
// check in Chromium, the capture itself) is always present. The blitz-dom sections are
// filled from the files the Rust probe and `compare.mjs . blitz` write, and say "not run"
// until those exist.
//
// usage: node assemble-results.mjs <experiment dir> [<load average while the probe ran>]

import fs from 'node:fs';
import path from 'node:path';

const [, , expDir, load] = process.argv;
const p = f => path.join(expDir, 'captured', f);
const read = f => fs.existsSync(p(f)) ? JSON.parse(fs.readFileSync(p(f), 'utf8')) : null;

const meta = read('meta.json');
const cssCheck = read('css-reconstruction.json');
const fixture = read('compare-chrome-fixture.json');
const blitz = read('compare-blitz.json');
const blitzErrors = read('blitz-css-errors.json');
const blitzMeta = read('blitz-meta.json');

function brief(cmp) {
	if (!cmp) return 'not run';
	return {
		regions: Object.fromEntries(Object.entries(cmp.regions).map(([k, v]) => [k, v.error ? v : {
			elementsCompared: v.elementsCompared,
			elementsExcludedBecauseTheFixtureDiffers: v.elementsExcludedBecauseTheFixtureDiffers,
			elementsMissing: v.elementsMissingFromCandidate,
			hiddenInVsCodeOnly: v.hiddenInVsCodeOnly,
			hiddenInCandidateOnly: v.hiddenInCandidateOnly,
			maxAbsErrorPx: v.maxAbsError,
			positionErrorPx: v.positionError,
			sizeErrorPx: v.sizeError,
			elementsWithin: v.elementsWithin,
			worst: v.worst.slice(0, 8),
			texts: {
				compared: v.texts.compared, missing: v.texts.missing, lineCountMismatches: v.texts.lineCountMismatches,
				widthErrorPx: v.texts.widthError, positionErrorPx: v.texts.positionError,
			},
		}])),
		text: cmp.text.error ? cmp.text : {
			cases: cmp.text.cases,
			sameLineBreaks: cmp.text.sameLineBreaks,
			wrappedCasesWithSameBreaks: cmp.text.wrappedCasesWithSameBreaks,
			singleLineWidthDiffPx: cmp.text.singleLineWidthDiff,
			singleLineWidthDiffPct: cmp.text.singleLineWidthDiffPct,
			maxCharXDiffPx: cmp.text.maxCharXDiff,
			boxHeightDiffPx: cmp.text.heightDiff,
			rows: cmp.text.rows,
		},
	};
}

const result = {
	question: 'GitHub issue #19: how faithfully blitz-dom lays out the VS Code workbench with its own CSS, and whether Parley text layout matches Chromium closely enough to paint at its positions',
	codeOss: {
		repository: 'https://github.com/microsoft/vscode',
		commit: '7debcd0e2acdea1c52de81bf9ee1620444407dda',
		why: 'the commit of the VS Code build installed on the machine (1.138.0), so the stylesheet and the DOM come from the same source',
		codicons: '@vscode/codicons 0.0.46-39 (the version Code-OSS locks), integrity checked against package-lock.json',
	},
	vscode: meta && {
		version: '1.138.0', userAgent: meta.userAgent, viewport: meta.viewport, htmlLang: meta.htmlAttrs?.lang,
		fontReportedByChromium: meta.platformFonts,
	},
	stylesheetReconstruction: cssCheck,
	fixtureCheckInChromium: brief(fixture),
	systemFontDefaultInstanceWithoutShaping: read('sf-default-instance.json'),
	blitz: {
		layout: brief(blitz),
		cssDroppedByStylo: blitzErrors ? {
			'workbench.css': { dropped: blitzErrors['workbench.css'].dropped, byKind: blitzErrors['workbench.css'].byKind, top: blitzErrors['workbench.css'].entries.slice(0, 40) },
			'runtime.css': { dropped: blitzErrors['runtime.css'].dropped, byKind: blitzErrors['runtime.css'].byKind, top: blitzErrors['runtime.css'].entries.slice(0, 20) },
		} : 'not run',
		fonts: blitzMeta ? blitzMeta.fonts : 'not run',
		timings: blitzMeta ? { ...blitzMeta.timings, loadAverageWhileRunning: load ?? 'not recorded' } : 'not run',
	},
};
fs.writeFileSync(path.join(expDir, 'results.json'), JSON.stringify(result, null, 1) + '\n');
console.log(`wrote ${path.join(expDir, 'results.json')}; blitz layout: ${blitz ? 'present' : 'not run'}`);
