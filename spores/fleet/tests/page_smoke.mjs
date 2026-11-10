// Does the fleet page actually draw the document the host serves?
//
//   node tests/page_smoke.mjs <scene.json>
//
// The page is the one host nobody can inspect by reading the scene, so it runs
// the real `<script>` out of `web/index.html` against a recording canvas and a
// `fetch` that answers with a real scene. A page that throws, draws nothing, or
// repaints a 304 shows up as a failed test instead of an empty window.
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

const drawn = { rects: 0, texts: [], blits: 0 };
const recorder = () => ({
  setTransform() {}, clearRect() {}, beginPath() {}, fill() {}, save() {}, restore() {},
  translate() {}, rotate() {},
  rect() { drawn.rects += 1; },
  roundRect() { drawn.rects += 1; },
  fillText(text, x, y) { drawn.texts.push({ text, x, y }); },
  fillStyle: '', font: '', textBaseline: '',
});
const blitter = { setTransform() {}, clearRect() {}, drawImage() { drawn.blits += 1; } };
const canvas = { width: 0, height: 0, style: {}, getContext: () => blitter };
const status = { textContent: 'loading' };

const scene = readFileSync(scene_path, 'utf8');
const ETAG = '"smoke"';
let requests = 0;
let tick = null;
const sandbox = {
  document: {
    getElementById: (id) => (id === 'scene' ? canvas : status),
    createElement: () => ({ width: 0, height: 0, getContext: () => recorder() }),
  },
  window: { devicePixelRatio: 2 },
  innerWidth: 1600,
  innerHeight: 1000,
  addEventListener() {},
  setInterval: (fn) => { tick = fn; return 0; },
  setTimeout: () => 0,
  clearTimeout() {},
  fetch: async (url, options) => {
    requests += 1;
    const revalidating = (options?.headers?.['if-none-match'] || null) === ETAG;
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
const settle = () => new Promise((resolve) => setImmediate(resolve));
await settle();

const problems = [];
const firstPaint = { rects: drawn.rects, texts: drawn.texts.length, blits: drawn.blits };
if (drawn.texts.length === 0) problems.push('the page drew no text');
if (drawn.rects === 0) problems.push('the page drew no rects');
if (drawn.blits === 0) problems.push('the page never blitted a frame');
if (drawn.rects < 100) problems.push(`the page drew too few marks: ${drawn.rects}`);
if (!/nodes/.test(status.textContent)) problems.push(`status is not a node count: ${status.textContent}`);
if (!drawn.texts.some((entry) => entry.text === 'FLEET')) problems.push('the masthead is missing');
if (!drawn.texts.some((entry) => entry.text === 'Mesh topology')) problems.push('the graph card is missing');

if (tick) {
  tick();
  await settle();
}
if (requests < 2) problems.push(`the poll never ran (${requests} requests)`);
if (drawn.blits !== firstPaint.blits || drawn.texts.length !== firstPaint.texts || drawn.rects !== firstPaint.rects) {
  problems.push(`the page repainted an unchanged scene: ${firstPaint.blits} paint(s) then ${drawn.blits}`);
}

for (const entry of drawn.texts) {
  if (!(entry.x >= -0.5 && entry.x <= 1600.5 && entry.y >= -0.5 && entry.y <= 1000.5)) {
    problems.push(`text outside the window: ${entry.text} at ${entry.x},${entry.y}`);
  }
}

if (problems.length) {
  console.error(problems.join('\n'));
  process.exit(1);
}
console.log(`page: ok — ${drawn.blits} blit, ${drawn.rects} rects, ${drawn.texts.length} texts`);
