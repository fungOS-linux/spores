// Does the browser host's page actually work — draw, route a press, say no?
//
//   node tests/page_smoke.mjs <scene.json>
//
// It runs the real `<script>` from `web/index.html` against a recording canvas
// and a fetch that answers with the scene `tools/shot.lua` produced. The page
// shipped in this repo once drew nothing and said so only in a status line, so
// drawing is asserted before anything else.
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const scene_path = process.argv[2];
if (!scene_path) {
  console.error('usage: node tests/page_smoke.mjs <scene.json>');
  process.exit(2);
}

const html = readFileSync(new URL('../web/index.html', import.meta.url), 'utf8');
const match = /<script>([\s\S]*?)<\/script>/.exec(html);
if (!match) {
  console.error('no <script> in web/index.html');
  process.exit(1);
}

const WIDTH = 1182, HEIGHT = 1569;
const sceneText = readFileSync(scene_path, 'utf8');
const scene = JSON.parse(sceneText);

const drawn = { rects: 0, texts: [], blits: 0 };
const recorder = () => ({
  setTransform() {}, clearRect() {}, beginPath() {}, fill() {},
  rect() { drawn.rects += 1; },
  roundRect() { drawn.rects += 1; },
  fillText(text, x, y) { drawn.texts.push({ text, x, y }); },
  fillStyle: '', font: '', textBaseline: '',
});
const blitter = { setTransform() {}, clearRect() {}, drawImage() { drawn.blits += 1; } };

const handlers = {};
const element = (extra = {}) => ({
  width: 0, height: 0, style: {}, value: '',
  getContext: () => blitter,
  addEventListener: (type, fn) => { handlers[type] = fn; },
  fire: (type, event) => { if (handlers[type]) handlers[type](event); },
  ...extra,
});

const canvas = element();
const hover = { style: {} };
const status = { textContent: 'loading' };
const notice = { textContent: '' };
const listeners = {};
const seen = [];
const applied = [];
let requests = 0;
let applyReply = { ok: true, message: 'saved schedule for hub-poll' };
let tick = null;
let version = 0;

const sandbox = {
  document: {
    getElementById: (id) => ({ scene: canvas, hover, status, notice })[id],
    createElement: () => ({ width: 0, height: 0, getContext: () => recorder() }),
  },
  window: { devicePixelRatio: 2 },
  location: { search: '' },
  innerWidth: WIDTH,
  innerHeight: HEIGHT,
  addEventListener: (type, fn) => { listeners[type] = fn; },
  setInterval: (fn) => { tick = fn; return 0; },
  setTimeout: (fn) => setTimeout(fn, 0),
  clearTimeout: (id) => clearTimeout(id),
  URLSearchParams, URL, JSON, Date, Math, parseInt, console,
  fetch: async (url, options = {}) => {
    if (String(url) === 'apply') {
      const body = JSON.parse(options.body);
      applied.push(body.intent);
      return { ok: true, status: 200, json: async () => applyReply };
    }
    requests += 1;
    const params = Object.fromEntries(new URL(url, 'https://pulse.myc/').searchParams);
    seen.push(params);
    // A scene tagged with the request's host state, so a state change produces
    // bytes the page has to draw instead of a body it can ignore.
    const copy = JSON.parse(sceneText);
    copy.nodes.push({
      id: `echo-${version}`, type: 'text', ink: 'muted', text: '',
      transform: { translation: [0, 0, 0], scale: [1, 1, 1] }, px: 1,
    });
    const body = JSON.stringify(copy);
    return { ok: true, status: 200, json: async () => JSON.parse(body), text: async () => body };
  },
};
sandbox.globalThis = sandbox;
vm.createContext(sandbox);
vm.runInContext(match[1], sandbox);

const settle = () => new Promise((resolve) => setTimeout(resolve, 10));
const problems = [];
const click = async (id) => {
  const press = (scene.presses || []).find((entry) => entry.id === id);
  if (!press) {
    problems.push(`the scene carries no press ${id}`);
    return;
  }
  canvas.fire('click', {
    clientX: (press.x + 4) / 8 * WIDTH + 2,
    clientY: (4 - press.y) / 8 * HEIGHT + 2,
  });
  await settle();
};

await settle();
if (drawn.texts.length === 0) problems.push('the page drew no text');
if (drawn.rects === 0) problems.push('the page drew no rects');
if (drawn.blits === 0) problems.push('the page never blitted a frame');
if (!/nodes/.test(status.textContent)) problems.push(`status is not a node count: ${status.textContent}`);
if (!drawn.texts.some((entry) => entry.text === 'PULSE')) problems.push('the masthead is missing');
if (!drawn.texts.some((entry) => entry.text === 'Nightly backup')) problems.push('the selected cron is missing');

// A poll that returns the same bytes must not repaint the canvas.
const before = { blits: drawn.blits, texts: drawn.texts.length };
if (tick) {
  tick();
  await settle();
}
if (drawn.blits !== before.blits || drawn.texts.length !== before.texts) {
  problems.push('the page repainted an unchanged scene');
}

// A preset press is a write: the page posts the intent and shows the answer.
version += 1;
await click('pulse-presets-hourly');
if (!applied.some((intent) => intent.kind === 'preset' && intent.preset === 'hourly')) {
  problems.push(`the preset press did not reach the host: ${JSON.stringify(applied)}`);
}
if (!/saved schedule/.test(notice.textContent)) {
  problems.push(`the apply result is not shown: ${JSON.stringify(notice.textContent)}`);
}
if (requests < 2) problems.push('the page did not ask for a new scene after an edit');

// Selecting a row is host state, not a write: the next scene asks for it.
version += 1;
await click('pulse-cron-2');
if (seen.at(-1)?.select !== 'hub-poll') {
  problems.push(`selecting a row did not reach the host: ${JSON.stringify(seen.at(-1))}`);
}
if (applied.length !== 1) problems.push('a selection wrote a record');

// A pane switch is also host state.
version += 1;
await click('pulse-tabs-ingress');
if (seen.at(-1)?.tab !== 'ingress') {
  problems.push(`the ingress tab did not reach the host: ${JSON.stringify(seen.at(-1))}`);
}

// Back to crons, so the refusal below lands on a control that exists in the
// scene the page currently has.
version += 1;
await click('pulse-tabs-crons');
if (seen.at(-1)?.tab !== 'crons') {
  problems.push(`the crons tab did not reach the host: ${JSON.stringify(seen.at(-1))}`);
}

// A refused edit is shown in the page's own words, not as a status code.
applyReply = { ok: false, problems: [{ field: 'target', text: 'not registered' }] };
version += 1;
await click('pulse-presets-5m');
if (!/target: not registered/.test(notice.textContent)) {
  problems.push(`the refusal is not shown: ${JSON.stringify(notice.textContent)}`);
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
console.log(`page: ok — ${drawn.blits} blits, ${drawn.texts.length} texts, ` +
  `${applied.length} intent(s) posted, ${requests} scene request(s)`);
