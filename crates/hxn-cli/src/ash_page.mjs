// Runs an Ash wasm program in a page: serves its directory with `ash serve`,
// which sends the cross-origin isolation headers a threaded module needs,
// opens it in headless Chrome over the DevTools protocol, and waits for the
// page to set its title to done or trapped. Prints what the program printed
// to stdout, the page's other lines to stderr, and exits with its status.
//
//     node ash_page.mjs <ash> <directory> <chrome> [chrome flags...]
//
// Node 22 or later, for its global fetch and WebSocket.
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";

const [ash, dir, chrome, ...flags] = process.argv.slice(2);
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// A port nothing listens on now.
const freePort = () =>
  new Promise((resolve, reject) => {
    const server = createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
  });

const webPort = await freePort();
const debugPort = await freePort();
const profile = mkdtempSync(join(tmpdir(), "hxn-ash-page-"));
const server = spawn(ash, ["serve", dir, "--port", String(webPort)], { stdio: "ignore" });
const browser = spawn(
  chrome,
  [
    "--headless=new",
    `--remote-debugging-port=${debugPort}`,
    `--user-data-dir=${profile}`,
    "--no-first-run",
    "--no-default-browser-check",
    ...flags,
    "about:blank",
  ],
  { stdio: "ignore" },
);
const cleanup = () => {
  server.kill();
  browser.kill();
  try {
    rmSync(profile, { recursive: true, force: true });
  } catch {}
};

let status = 1;
try {
  let target;
  for (let i = 0; i < 150 && !target; i++) {
    try {
      const response = await fetch(
        `http://127.0.0.1:${debugPort}/json/new?http://localhost:${webPort}/`,
        { method: "PUT" },
      );
      if (response.ok) target = await response.json();
    } catch {}
    if (!target) await sleep(200);
  }
  if (!target) throw new Error(`${chrome} did not start`);

  const socket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.onopen = resolve;
    socket.onerror = () => reject(new Error("the DevTools connection failed"));
  });
  let nextId = 1;
  const waiting = new Map();
  socket.onmessage = (event) => {
    const message = JSON.parse(event.data);
    if (message.id && waiting.has(message.id)) {
      waiting.get(message.id)(message);
      waiting.delete(message.id);
    }
  };
  const call = (method, params = {}) =>
    new Promise((resolve) => {
      const id = nextId++;
      waiting.set(id, resolve);
      socket.send(JSON.stringify({ id, method, params }));
    });

  // The page's log, line by line, with each line's class: none for what the
  // program wrote to stdout, `err` for stderr, `meta` for the page's own.
  const read = `JSON.stringify({
    title: document.title,
    lines: [...(document.getElementById("out")?.children ?? [])].map((s) => [s.className, s.textContent]),
  })`;
  const timeout = Number(process.env.HXN_PAGE_TIMEOUT ?? 300) * 1000;
  const deadline = Date.now() + timeout;
  let page = { title: "", lines: [] };
  while (Date.now() < deadline) {
    const state = await call("Runtime.evaluate", { expression: read, returnByValue: true });
    try {
      page = JSON.parse(state.result.result.value);
      if (/ — (done|trapped)$/.test(page.title)) break;
    } catch {}
    await sleep(250);
  }
  // Ash's runtime writes its own notes to the program's stdout, prefixed
  // `[ash] `; they go to stderr, as under `ash run`.
  for (const [kind, text] of page.lines) {
    if (kind === "" && !text.startsWith("[ash] ")) process.stdout.write(text);
    else process.stderr.write(text);
  }
  const exited = page.lines.map(([, text]) => /^exited with (-?\d+)/.exec(text)).find(Boolean);
  if (/ — done$/.test(page.title)) status = exited ? Number(exited[1]) : 0;
  else if (!/ — trapped$/.test(page.title))
    process.stderr.write(`the page did not finish in ${timeout / 1000} s (HXN_PAGE_TIMEOUT)\n`);
} catch (e) {
  process.stderr.write(`${e.message}\n`);
} finally {
  cleanup();
}
process.exit(status);
