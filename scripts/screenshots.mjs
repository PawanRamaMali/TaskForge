// Capture the README screenshots from the dev server in demo mode.
// Usage: start `npm run dev` in another terminal, then `node scripts/screenshots.mjs`.
// Drives headless Edge or Chrome over the DevTools protocol so each shot gets an
// exact viewport (plain --window-size has a minimum width that breaks the mini
// view). Set BROWSER_PATH to pick another Chromium-based browser. Needs Node 22+.

import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const base = process.env.DEMO_URL ?? 'http://localhost:1420/';
const outDir = resolve('docs/screenshots');

const shots = [
  { file: 'dashboard.png', query: 'demo', width: 1280, height: 820, scale: 1 },
  { file: 'stability-check.png', query: 'demo&open=stability', width: 1280, height: 900, scale: 1 },
  { file: 'settings.png', query: 'demo&open=settings', width: 1280, height: 900, scale: 1 },
  { file: 'mini-view.png', query: 'demo&view=mini', width: 280, height: 224, scale: 2 },
];

const candidates = [
  process.env.BROWSER_PATH,
  'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
  'C:/Program Files/Microsoft/Edge/Application/msedge.exe',
  'C:/Program Files/Google/Chrome/Application/chrome.exe',
  '/usr/bin/google-chrome',
  '/usr/bin/chromium',
  '/usr/bin/chromium-browser',
  '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
].filter(Boolean);
const browserPath = candidates.find((p) => existsSync(p));
if (!browserPath) {
  console.error('No Edge or Chrome found. Set BROWSER_PATH to a Chromium-based browser.');
  process.exit(1);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// A separate profile keeps this away from any browser window that is already open.
const profile = join(tmpdir(), 'taskforge-screenshots');
const portFile = join(profile, 'DevToolsActivePort');
rmSync(portFile, { force: true });

const browser = spawn(
  browserPath,
  ['--headless=new', '--disable-gpu', '--hide-scrollbars', '--remote-debugging-port=0', `--user-data-dir=${profile}`, 'about:blank'],
  { stdio: 'ignore' }
);

async function devtoolsUrl() {
  for (let i = 0; i < 100; i++) {
    if (existsSync(portFile)) {
      const [port, path] = readFileSync(portFile, 'utf8').trim().split('\n');
      if (port && path) return `ws://127.0.0.1:${port}${path}`;
    }
    await sleep(100);
  }
  throw new Error('the browser did not start its DevTools endpoint');
}

const ws = new WebSocket(await devtoolsUrl());
await new Promise((res, rej) => {
  ws.onopen = res;
  ws.onerror = rej;
});

let nextId = 0;
const pending = new Map();
const waiters = [];
ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  if (msg.id && pending.has(msg.id)) {
    const { resolve: ok, reject } = pending.get(msg.id);
    pending.delete(msg.id);
    if (msg.error) reject(new Error(msg.error.message));
    else ok(msg.result);
  } else if (msg.method) {
    for (const w of [...waiters]) {
      if (w.method === msg.method && w.sessionId === msg.sessionId) {
        waiters.splice(waiters.indexOf(w), 1);
        w.resolve(msg.params);
      }
    }
  }
};

function send(method, params = {}, sessionId) {
  const id = ++nextId;
  ws.send(JSON.stringify({ id, method, params, sessionId }));
  return new Promise((ok, reject) => pending.set(id, { resolve: ok, reject }));
}

function waitFor(method, sessionId) {
  return new Promise((ok) => waiters.push({ method, sessionId, resolve: ok }));
}

try {
  mkdirSync(outDir, { recursive: true });
  const { targetId } = await send('Target.createTarget', { url: 'about:blank' });
  const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true });
  await send('Page.enable', {}, sessionId);

  for (const shot of shots) {
    await send(
      'Emulation.setDeviceMetricsOverride',
      { width: shot.width, height: shot.height, deviceScaleFactor: shot.scale, mobile: false },
      sessionId
    );
    const loaded = waitFor('Page.loadEventFired', sessionId);
    await send('Page.navigate', { url: `${base}?${shot.query}` }, sessionId);
    await loaded;
    // Let the demo fill its graphs and open any dialog.
    await sleep(3000);
    const { data } = await send('Page.captureScreenshot', { format: 'png' }, sessionId);
    const out = join(outDir, shot.file);
    writeFileSync(out, Buffer.from(data, 'base64'));
    console.log(`wrote ${out}`);
  }
  await send('Browser.close').catch(() => {});
} finally {
  ws.close();
  browser.kill();
}
