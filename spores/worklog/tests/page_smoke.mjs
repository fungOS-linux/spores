// Does the browser page actually draw the document the host serves?
//
//   node tests/page_smoke.mjs <scene.json>
//
// The page shipped once without ever being executed — it read `scene.nodes` off a
// document that was a bare array — and the only thing that noticed was the
// person looking at an empty window. This runs the real `<script>` out of
// `web/index.html` against a recording canvas and a `fetch` that answers with a
// real scene, so a page that throws shows up as a failed test instead.
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const scene_path = process.argv[2];
if (!scene_path) {
  console.error('usage: node tests/page_smoke.mjs <scene.json>');
  process.exit(2);
}

const here = new URL('.', import.meta.url);
const html = readFileSync(new URL('../web/index.html', here), 'utf8');
const match = /<script>([\s\S]*?)<\/script>/.exec(html);
if (!match) {
  console.error('no <script> in web/index.html');
  process.exit(1);
}

const drawn = { rects: 0, texts: [] };
const context = {
  setTransform() {}, clearRect() {}, beginPath() {}, fill() {},
  rect() { drawn.rects += 1; },
  roundRect() { drawn.rects += 1; },
  fillText(text, x, y) { drawn.texts.push({ text, x, y }); },
  fillStyle: '', font: '', textBaseline: '',
};
const canvas = { width: 0, height: 0, style: {}, getContext: () => context };
const status = { textContent: 'loading' };

const scene = readFileSync(scene_path, 'utf8');
const ETAG = '"smoke"';
let requests = 0;
let tick = null;
const sandbox = {
  document: { getElementById: (id) => (id === 'scene' ? canvas : status) },
  window: { devicePixelRatio: 2 },
  innerWidth: 1182,
  innerHeight: 1569,
  addEventListener() {},
  // The page polls; the test drives the poll by hand instead of waiting.
  setInterval: (fn) => { tick = fn; return 0; },
  setTimeout: () => 0,
  clearTimeout() {},
  fetch: async (url, options) => {
    requests += 1;
    const revalidating = (options?.headers?.['if-none-match'] || null) === ETAG;
    // The second request revalidates, which is what a poll of an unchanged scene
    // looks like: a 304 and nothing to draw.
    if (revalidating) {
      return {
        ok: true, status: 304,
        headers: { get: (name) => (name.toLowerCase() === 'etag' ? ETAG : null) },
        json: async () => { throw new Error('304 has no body'); },
        text: async () => '',
      };
    }
    return {
      ok: true, status: 200,
      headers: { get: (name) => (name.toLowerCase() === 'etag' ? ETAG : null) },
      json: async () => JSON.parse(scene),
      text: async () => '',
    };
  },
  console, Date, Math, parseInt, JSON,
};
sandbox.globalThis = sandbox;
vm.createContext(sandbox);
vm.runInContext(match[1], sandbox);
// `refresh()` is async: let its fetch and draw settle before looking.
const settle = () => new Promise((resolve) => setImmediate(resolve));
await settle();

const problems = [];
const firstPaint = { rects: drawn.rects, texts: drawn.texts.length };
if (drawn.texts.length === 0) problems.push('the page drew no text');
if (drawn.rects === 0) problems.push('the page drew no rects');
if (!/nodes/.test(status.textContent)) problems.push(`status is not a node count: ${status.textContent}`);
if (!drawn.texts.some((entry) => entry.text === 'WORKLOG')) {
  problems.push('the masthead is missing');
}

// Polling an unchanged scene must not repaint it. A page that clears and redraws
// on every 304 is the flicker this guards: the second pass would record a second
// copy of every node.
if (tick) {
  tick();
  await settle();
}
if (requests < 2) problems.push(`the poll never ran (${requests} requests)`);
if (drawn.texts.length !== firstPaint.texts || drawn.rects !== firstPaint.rects) {
  problems.push(
    `the page repainted an unchanged scene: ${firstPaint.rects}/${firstPaint.texts} then ` +
    `${drawn.rects}/${drawn.texts.length}`);
}

// A rect drawn at the far edge, or text off the surface, is the same class of
// defect the Lua fixture guards: the browser is the one host nobody can inspect
// by reading the scene.
for (const entry of drawn.texts) {
  if (!(entry.x >= -0.5 && entry.x <= 1182.5 && entry.y >= -0.5 && entry.y <= 1569.5)) {
    problems.push(`text outside the window: ${entry.text} at ${entry.x},${entry.y}`);
  }
}

if (problems.length) {
  console.error(problems.join('\n'));
  process.exit(1);
}
console.log(`page: ok — ${drawn.rects} rects, ${drawn.texts.length} texts`);
