// Does the browser page draw the document the host serves — and can it navigate?
//
//   node tests/page_smoke.mjs <scene.json>
//
// The page shipped once without ever being executed in the worklog Spore, and a
// scene shape change went unnoticed until someone looked at an empty window.
// This runs the real `<script>` out of `web/index.html` against a recording
// canvas, a `fetch` that answers with a real scene, and a click at the centre of
// a real hit region, so a page that draws but cannot route a press shows up as a
// failed test instead.
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

const scene = JSON.parse(readFileSync(scene_path, 'utf8'));
const WIDTH = 1182;
const HEIGHT = 820;

const drawn = { rects: 0, texts: [], blits: 0 };
const recorder = () => ({
  setTransform() {}, clearRect() {}, beginPath() {}, fill() {},
  rect() { drawn.rects += 1; },
  roundRect() { drawn.rects += 1; },
  fillText(text, x, y) { drawn.texts.push({ text, x, y }); },
  fillStyle: '', font: '', textBaseline: '',
});
const blitter = { setTransform() {}, clearRect() {}, drawImage() { drawn.blits += 1; } };
const listeners = {};
const canvas = {
  width: 0, height: 0, style: {},
  getContext: () => blitter,
  getBoundingClientRect: () => ({ left: 0, top: 0, width: WIDTH, height: HEIGHT }),
  addEventListener: (name, fn) => { listeners[name] = fn; },
};
const status = { textContent: 'loading' };

const ETAG = '"smoke"';
const requests = [];
let tick = null;
const sandbox = {
  document: {
    getElementById: (id) => (id === 'scene' ? canvas : status),
    createElement: () => ({ width: 0, height: 0, getContext: () => recorder() }),
  },
  window: { devicePixelRatio: 1 },
  innerWidth: WIDTH,
  innerHeight: HEIGHT,
  location: { search: '' },
  history: { replaceState() {} },
  addEventListener() {},
  // The page polls; the test drives the poll by hand instead of waiting.
  setInterval: (fn) => { tick = fn; return 0; },
  setTimeout: () => 0,
  clearTimeout() {},
  URLSearchParams,
  fetch: async (url, options) => {
    const parsed = new URLSearchParams(String(url).split('?')[1] || '');
    requests.push({ url: String(url), path: parsed.get('path'), root: parsed.get('root') });
    // A navigation asks for a path; an unchanged poll revalidates. Those are the
    // two shapes the page has to tell apart.
    const navigating = parsed.get('path') !== null;
    const revalidating = (options?.headers?.['if-none-match'] || null) === ETAG;
    if (revalidating && !navigating) {
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
      json: async () => JSON.parse(JSON.stringify(scene)),
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
const firstPaint = { rects: drawn.rects, texts: drawn.texts.length, blits: drawn.blits };
if (drawn.texts.length === 0) problems.push('the page drew no text');
if (drawn.rects === 0) problems.push('the page drew no rects');
if (drawn.blits === 0) problems.push('the page never blitted a frame');
if (!/nodes/.test(status.textContent)) problems.push(`status is not a node count: ${status.textContent}`);
for (const expected of ['files', 'assets/', '68.4 GB']) {
  if (!drawn.texts.some((entry) => entry.text.includes(expected))) {
    problems.push(`the page did not draw ${expected}`);
  }
}

// Polling an unchanged scene must not repaint it.
if (tick) {
  tick();
  await settle();
}
if (requests.length < 2) problems.push(`the poll never ran (${requests.length} requests)`);
if (drawn.blits !== firstPaint.blits || drawn.texts.length !== firstPaint.texts || drawn.rects !== firstPaint.rects) {
  problems.push(`the page repainted an unchanged scene: ${firstPaint.blits} paint(s) then ${drawn.blits}`);
}

// A press on a directory row must become a request for that directory. The
// target is a real hit from the real scene, not a coordinate guessed here.
const hits = Array.isArray(scene.hits) ? scene.hits : [];
const target = hits.find((hit) => hit.action === 'files-open' && hit.body && hit.body.path);
if (!target) {
  problems.push('the scene has no pressable directory row');
} else if (!listeners.click) {
  problems.push('the page installed no click handler');
} else {
  const before = requests.length;
  const clientX = ((target.x + target.w / 2) + 4) / 8 * WIDTH;
  const clientY = (4 - (target.y - target.h / 2)) / 8 * HEIGHT;
  listeners.click({ clientX, clientY });
  await settle();
  const next = requests[before];
  if (!next) {
    problems.push('a click did not ask for a scene');
  } else if (next.path !== target.body.path) {
    problems.push(`a click asked for ${next.path}, not ${target.body.path}`);
  } else if (next.root !== target.body.root) {
    problems.push(`a click asked for root ${next.root}, not ${target.body.root}`);
  }
}

// No text outside the window: the browser is the one host nobody can inspect by
// reading the scene.
for (const entry of drawn.texts) {
  if (entry.x < -0.5 || entry.x > WIDTH + 0.5 || entry.y < -0.5 || entry.y > HEIGHT + 0.5) {
    problems.push(`text outside the window: ${entry.text} at ${entry.x},${entry.y}`);
  }
}

if (problems.length) {
  console.error(problems.join('\n'));
  process.exit(1);
}
console.log(`page: ok — ${drawn.blits} blit, ${drawn.rects} rects, ${drawn.texts.length} texts, ${hits.length} hits`);
