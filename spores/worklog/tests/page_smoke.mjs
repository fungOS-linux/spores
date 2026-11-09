// Does the browser page actually work — start the rasterizer, draw, search, hover,
// scroll?
//
//   node tests/page_smoke.mjs <scene.json>
//
// The page shipped once without ever being executed, so this runs the real
// `<script>` out of `web/index.html` against a stub canvas, a stub font store and
// a fake wasm renderer that answers with real RGBA. It checks the things a page
// can get wrong without anyone noticing: never starting the renderer, painting
// instead of delegating, drawing again when nothing changed, and losing an
// interaction between the pointer and the host.
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
const FONT_ID = 'deadbeef';
const sceneText = readFileSync(scene_path, 'utf8');
const scene = JSON.parse(sceneText);
const ETAG = '"smoke"';

// What the page asked the rasterizer for, in order. The renderer is the thing
// under test from the page's side: if this stays empty, the page is painting a
// scene itself again.
const rendered = [];
const problems = [];
let blits = 0, pasted = 0, fontFetches = 0, fontCachePuts = 0, cacheHits = 0, createdWith = 0;

const element = (extra = {}) => {
  const handlers = {};
  return {
    width: 0, height: 0, style: {}, value: '', textContent: '',
    getContext: () => ({
      setTransform() {},
      clearRect() {},
      drawImage() { blits += 1; },
      putImageData() { pasted += 1; },
    }),
    addEventListener: (type, fn) => { handlers[type] = fn; },
    fire: (type, event) => { if (handlers[type]) handlers[type](event); },
    ...extra,
  };
};

const canvas = element();
const input = element();
const status = element({ textContent: 'loading' });
const listeners = {};
const seen = [];
let requests = 0;
let tick = null;

// A scene that echoes the host state back, so a request that carried a query, a
// hover or an offset produces a document the renderer can be asked about.
const echo = (params) => {
  const copy = JSON.parse(sceneText);
  copy.nodes.push({
    id: `echo-${params.q || ''}-${params.hover || ''}-${params.offset || ''}`,
    type: 'text', ink: 'muted', text: '',
    transform: { translation: [0, 0, 0], scale: [1, 1, 1] }, px: 1,
  });
  return JSON.stringify(copy);
};

class ImageDataShim {
  constructor(data, width, height) {
    this.data = data;
    this.width = width;
    this.height = height;
  }
}

class FakeRenderer {
  static async create(bytes) {
    createdWith = bytes.length;
    return new FakeRenderer();
  }
  set_background() {}
  set_body() {}
  async render(document, width, height) {
    rendered.push({ document, width, height });
    // One lit pixel, so "the page blitted something" is not the same claim as
    // "the page blitted an empty buffer".
    const rgba = new Uint8ClampedArray(width * height * 4);
    rgba[0] = 33;
    return rgba;
  }
}

const sandbox = {
  document: {
    getElementById: (id) => ({ scene: canvas, q: input, status })[id],
    createElement: () => element(),
  },
  window: { devicePixelRatio: 2 },
  innerWidth: WIDTH,
  innerHeight: HEIGHT,
  addEventListener: (type, fn) => { listeners[type] = fn; },
  setInterval: (fn) => { tick = fn; return 0; },
  setTimeout: (fn) => setTimeout(fn, 0),
  clearTimeout: (id) => clearTimeout(id),
  URLSearchParams, URL, JSON, Date, Math, parseInt, console, Uint8ClampedArray,
  ImageData: ImageDataShim,
  // A cache that keeps what it is given, so a second start would be a hit. The
  // page opens one store and asks for the font by its content id.
  caches: {
    open: async () => {
      const store = new Map();
      return {
        match: async (url) => {
          const hit = store.get(url);
          if (hit) cacheHits += 1;
          return hit;
        },
        put: async (url, response) => { fontCachePuts += 1; store.set(url, response); },
      };
    },
  },
  fetch: async (url, options) => {
    const absolute = new URL(url, 'https://worklog.myc/');
    if (absolute.pathname === '/fonts.json') {
      return {
        ok: true, status: 200,
        json: async () => ({ id: FONT_ID, url: `/font/${FONT_ID}`, kind: 'font/ttf' }),
      };
    }
    if (absolute.pathname === `/font/${FONT_ID}`) {
      fontFetches += 1;
      return {
        ok: true, status: 200,
        clone() { return this; },
        arrayBuffer: async () => new Uint8Array([0, 1, 2, 3]).buffer,
      };
    }
    requests += 1;
    const params = Object.fromEntries(absolute.searchParams);
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

// The page imports the wasm glue dynamically, which is how the module stays out
// of the render path until it is asked for. Handing it a synthetic module keeps
// the test honest about the boundary: everything below it is the page's job.
const glue = new vm.SyntheticModule(['default', 'SceneRenderer'], function () {
  this.setExport('default', async () => {});
  this.setExport('SceneRenderer', FakeRenderer);
}, { context: sandbox });

vm.runInContext(match[1], sandbox, {
  importModuleDynamically: async (specifier) => {
    if (!specifier.includes('scene.js')) throw new Error(`unexpected import ${specifier}`);
    await glue.link(() => {});
    glue.evaluate();
    return glue;
  },
});

const settle = () => new Promise((resolve) => setTimeout(resolve, 20));

// The page starts the renderer before it asks for anything: font, module, device.
for (let attempt = 0; attempt < 20 && rendered.length === 0; attempt += 1) await settle();

if (createdWith === 0) problems.push('the page never created a renderer');
if (fontFetches !== 1) problems.push(`the font was fetched ${fontFetches} time(s), not once`);
if (fontCachePuts !== 1) problems.push('the font was not cached under its content id');
if (rendered.length === 0) problems.push('the page never asked the rasterizer for a frame');
if (rendered[0] && !rendered[0].document.includes('WORKLOG')) {
  problems.push('the first frame was not the served scene');
}
if (pasted === 0) problems.push('the page never painted the rendered pixels');
if (blits === 0) problems.push('the page never blitted a frame');
if (!/nodes/.test(status.textContent)) problems.push(`status is not a node count: ${status.textContent}`);

// Polling an unchanged scene must not repaint it, whichever way the poll comes
// back. A page that clears and redraws on a repeat is the flicker this guards.
const firstFrames = rendered.length, firstBlits = blits;
if (tick) {
  tick();
  await settle();
  tick();
  await settle();
}
if (requests < 3) problems.push(`the poll never ran (${requests} requests)`);
if (rendered.length !== firstFrames || blits !== firstBlits) {
  problems.push(`the page repainted an unchanged scene: ${firstBlits} blit(s) then ${blits}`);
}

// Search: typing asks the host for the query, and the answer is rendered.
const beforeSearch = rendered.length;
input.value = 'soil';
input.fire('input');
await settle();
if (seen.at(-1)?.q !== 'soil') problems.push(`typing did not reach the host: ${JSON.stringify(seen.at(-1))}`);
if (rendered.length <= beforeSearch) problems.push('a search result was not rendered');
if (rendered.at(-1) && !rendered.at(-1).document.includes('echo-soil')) {
  problems.push('the rendered frame is not the answer to the query');
}

// Hover: the pointer names a row, the host returns the frame with it lit.
const row = (scene.hits || []).find((hit) => /^event-\d+$/.test(hit.id));
if (!row) {
  problems.push('the scene carries no row hit targets');
} else {
  const beforeHover = rendered.length;
  canvas.fire('mousemove', {
    clientX: (row.x + 4) / 8 * WIDTH + 4,
    clientY: (4 - row.y) / 8 * HEIGHT + 4,
  });
  await settle();
  if (seen.at(-1)?.hover !== row.id) {
    problems.push(`hovering a row did not report it: ${JSON.stringify(seen.at(-1))}`);
  }
  if (rendered.length <= beforeHover) problems.push('a hovered row was not rendered');
}

// Scroll: a wheel notch is pixels, and it goes to the host that windows the log.
// A log that fits on one page has nowhere to go, which is also worth asserting.
const scrollable = Number(scene.meta?.total) > Number(scene.meta?.shown);
const beforeScroll = seen.length;
canvas.fire('wheel', { deltaY: 210, preventDefault() {} });
await settle();
if (scrollable) {
  if (Number(seen.at(-1)?.offset) !== 210) {
    problems.push(`the wheel did not scroll: ${JSON.stringify(seen.at(-1))}`);
  }
} else if (seen.length !== beforeScroll) {
  problems.push('the wheel scrolled a log that fits on one page');
}

// The renderer is asked for the surface it draws into, not for whatever the
// window happened to be: a frame at the wrong extent is a frame that gets scaled.
for (const frame of rendered) {
  if (frame.width !== WIDTH || frame.height !== HEIGHT) {
    problems.push(`a frame was rendered at ${frame.width}x${frame.height}, not ${WIDTH}x${HEIGHT}`);
  }
}

if (problems.length) {
  console.error(problems.join('\n'));
  process.exit(1);
}
console.log(`page: ok — ${rendered.length} frames through the rasterizer, ${blits} blits, ` +
  `search · hover · ${scrollable ? 'scroll' : 'no-scroll'} round-trip`);
