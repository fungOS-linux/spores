// Does the browser page actually work — draw, search, hover, scroll?
//
//   node tests/page_smoke.mjs <scene.json>
//
// The page shipped once without ever being executed, so this runs the real
// `<script>` out of `web/index.html` against a recording canvas, a stub input and
// a `fetch` that answers with a real scene. It checks the two things a page can
// get wrong without anyone noticing: drawing nothing, and drawing again when
// nothing changed.
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

const WIDTH = 1182, HEIGHT = 1569;
const sceneText = readFileSync(scene_path, 'utf8');
const scene = JSON.parse(sceneText);
const ETAG = '"smoke"';

const drawn = { rects: 0, texts: [], blits: 0 };
const recorder = () => ({
  setTransform() {}, clearRect() {}, beginPath() {}, fill() {},
  rect() { drawn.rects += 1; },
  roundRect() { drawn.rects += 1; },
  fillText(text, x, y) { drawn.texts.push({ text, x, y }); },
  fillStyle: '', font: '', textBaseline: '',
});
// The page paints off-screen and blits; a blit is one repaint, so counting them
// is the clearest answer to "did this poll redraw anything".
const blitter = { setTransform() {}, clearRect() {}, drawImage() { drawn.blits += 1; } };

function element(extra = {}) {
  const handlers = {};
  return {
    width: 0, height: 0, style: {}, value: '',
    getContext: () => blitter,
    addEventListener: (type, fn) => { handlers[type] = fn; },
    fire: (type, event) => { if (handlers[type]) handlers[type](event); },
    ...extra,
  };
}

const canvas = element();
const input = element();
const status = { textContent: 'loading' };
const listeners = {};
const seen = [];
let requests = 0;
let tick = null;

// A scene that echoes the host state back, so a request that carried a query,
// a hover or an offset produces different bytes to draw.
const echo = (params) => {
  const copy = JSON.parse(sceneText);
  copy.nodes.push({
    id: `echo-${params.q || ''}-${params.hover || ''}-${params.offset || ''}`,
    type: 'text', ink: 'muted', text: '',
    transform: { translation: [0, 0, 0], scale: [1, 1, 1] }, px: 1,
  });
  return JSON.stringify(copy);
};

const sandbox = {
  document: {
    getElementById: (id) => ({ scene: canvas, q: input, status })[id],
    createElement: () => ({ width: 0, height: 0, getContext: () => recorder() }),
  },
  window: { devicePixelRatio: 2 },
  innerWidth: WIDTH,
  innerHeight: HEIGHT,
  addEventListener: (type, fn) => { listeners[type] = fn; },
  // The page polls and debounces; the test drives both instead of waiting.
  setInterval: (fn) => { tick = fn; return 0; },
  setTimeout: (fn, ms) => setTimeout(fn, 0),
  clearTimeout: (id) => clearTimeout(id),
  URLSearchParams, URL, JSON, Date, Math, parseInt, console,
  fetch: async (url, options) => {
    requests += 1;
    const params = Object.fromEntries(new URL(url, 'https://worklog.myc/').searchParams);
    seen.push(params);
    const headers = { get: (name) => (name.toLowerCase() === 'etag' ? ETAG : null) };
    if (requests === 2) {
      // The revalidated shape: a 304 with nothing to draw.
      return {
        ok: true, status: 304, headers,
        json: async () => { throw new Error('304 has no body'); },
        text: async () => '',
      };
    }
    // The third poll is a 200 carrying the bytes already on screen — the shape
    // that shipped flickering, when the browser dropped the conditional header.
    // From the fourth request on the scene echoes the host state, so an
    // interaction produces something new to draw.
    const body = requests >= 4 ? echo(params) : sceneText;
    return { ok: true, status: 200, headers, json: async () => JSON.parse(body), text: async () => body };
  },
};
sandbox.globalThis = sandbox;
vm.createContext(sandbox);
vm.runInContext(match[1], sandbox);

const settle = () => new Promise((resolve) => setTimeout(resolve, 10));
await settle();

const problems = [];
const first = { rects: drawn.rects, texts: drawn.texts.length, blits: drawn.blits };
if (drawn.texts.length === 0) problems.push('the page drew no text');
if (drawn.rects === 0) problems.push('the page drew no rects');
if (drawn.blits === 0) problems.push('the page never blitted a frame');
if (!/nodes/.test(status.textContent)) problems.push(`status is not a node count: ${status.textContent}`);
if (!drawn.texts.some((entry) => entry.text === 'WORKLOG')) problems.push('the masthead is missing');

// Polling an unchanged scene must not repaint it, whichever way the poll comes
// back. A page that clears and redraws on a repeat is the flicker this guards.
if (tick) {
  tick();
  await settle();
  tick();
  await settle();
}
if (requests < 3) problems.push(`the poll never ran (${requests} requests)`);
if (drawn.blits !== first.blits || drawn.texts.length !== first.texts || drawn.rects !== first.rects) {
  problems.push(`the page repainted an unchanged scene: ${first.blits} paint(s) then ${drawn.blits}`);
}

// Search: typing asks the host for the query, and the answer is drawn.
input.value = 'soil';
input.fire('input');
await settle();
if (seen.at(-1)?.q !== 'soil') problems.push(`typing did not reach the host: ${JSON.stringify(seen.at(-1))}`);
if (drawn.blits !== first.blits + 1) problems.push('a search result was not drawn');

// Hover: the pointer names a row, the host returns the frame with it lit.
const row = (scene.hits || []).find((hit) => /^event-\d+$/.test(hit.id));
if (!row) {
  problems.push('the scene carries no row hit targets');
} else {
  canvas.fire('mousemove', {
    clientX: (row.x + 4) / 8 * WIDTH + 4,
    clientY: (4 - row.y) / 8 * HEIGHT + 4,
  });
  await settle();
  if (seen.at(-1)?.hover !== row.id) {
    problems.push(`hovering a row did not report it: ${JSON.stringify(seen.at(-1))}`);
  }
}

// Scroll: a wheel notch is pixels, and it goes to the host that windows the log.
// A log that fits on one page has nowhere to go, which is also worth asserting.
const scrollable = Number(scene.meta?.total) > Number(scene.meta?.shown);
const before = seen.length;
canvas.fire('wheel', { deltaY: 210, preventDefault() {} });
await settle();
if (scrollable) {
  if (Number(seen.at(-1)?.offset) !== 210) {
    problems.push(`the wheel did not scroll: ${JSON.stringify(seen.at(-1))}`);
  }
} else if (seen.length !== before) {
  problems.push('the wheel scrolled a log that fits on one page');
}

for (const entry of drawn.texts) {
  if (!(entry.x >= -0.5 && entry.x <= WIDTH + 0.5 && entry.y >= -0.5 && entry.y <= HEIGHT + 0.5)) {
    problems.push(`text outside the window: ${entry.text} at ${entry.x},${entry.y}`);
  }
}

if (problems.length) {
  console.error(problems.join('\n'));
  process.exit(1);
}
console.log(`page: ok — ${drawn.blits} blits, ${drawn.rects} rects, ${drawn.texts.length} texts, ` +
  `search · hover · ${scrollable ? 'scroll' : 'no-scroll'} round-trip`);
