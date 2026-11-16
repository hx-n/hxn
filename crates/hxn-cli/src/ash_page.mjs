// Runs an Ash wasm program in a web page: serves its directory with `ash serve`,
// which sends the cross-origin isolation headers a threaded module needs,
// opens it in headless Chrome over the DevTools protocol, and waits for the
// page to set its title to done or trapped. What the program writes reaches
// the console of the Worker it runs in: console.log for stdout, console.error
// for stderr. This prints both, from the page and every Worker it starts, in
// the order they were written, and exits 0 when the page says done, 1 when it
// trapped or did not finish.
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
// What goes to stdout and stderr, in the order it was written, printed once the run is over.
const lines = [];
const out = (text) => lines.push({ error: false, text });
const err = (text) => lines.push({ error: true, text });
try {
  let target;
  for (let i = 0; i < 150 && !target; i++) {
    try {
      const response = await fetch(`http://127.0.0.1:${debugPort}/json/new?about:blank`, {
        method: "PUT",
      });
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
  // Each console call: when, whether it was an error, and its text.
  const written = [];
  // The type of each target followed: the program's Worker and the others.
  const attached = [];
  const send = (method, params = {}, sessionId) =>
    new Promise((resolve) => {
      const id = nextId++;
      waiting.set(id, resolve);
      socket.send(JSON.stringify(sessionId ? { id, method, params, sessionId } : { id, method, params }));
    });
  // Every Worker, the program's and its agents', is attached before it runs
  // and paused until its console is followed.
  const follow = (sessionId) => {
    send("Runtime.enable", {}, sessionId);
    send("Target.setAutoAttach", { autoAttach: true, waitForDebuggerOnStart: true, flatten: true }, sessionId);
    send("Runtime.runIfWaitingForDebugger", {}, sessionId);
  };
  socket.onmessage = (event) => {
    const message = JSON.parse(event.data);
    if (message.id && waiting.has(message.id)) {
      waiting.get(message.id)(message);
      waiting.delete(message.id);
    } else if (message.method === "Target.attachedToTarget") {
      attached.push(message.params.targetInfo?.type ?? "?");
      follow(message.params.sessionId);
    } else if (message.method === "Runtime.consoleAPICalled") {
      const { type, args, timestamp } = message.params;
      const text = args.map((a) => (a.value !== undefined ? String(a.value) : a.description ?? "")).join(" ");
      written.push({ timestamp, error: type === "error" || type === "warning", text });
    } else if (message.method === "Runtime.exceptionThrown") {
      // Uncaught, in the web page or any Worker: a browser agent that fails to start, say.
      const { timestamp, exceptionDetails } = message.params;
      const text = exceptionDetails.exception?.description ?? exceptionDetails.text;
      written.push({ timestamp, error: true, text: `uncaught: ${text}` });
    }
  };
  await send("Runtime.enable");
  await send("Target.setAutoAttach", { autoAttach: true, waitForDebuggerOnStart: true, flatten: true });
  // HXN_PAGE_CORES sets navigator.hardwareConcurrency, which sizes the web
  // page's pool of thread agents: a smaller machine's, reproduced on this one.
  if (process.env.HXN_PAGE_CORES)
    await send("Emulation.setHardwareConcurrencyOverride", {
      hardwareConcurrency: Number(process.env.HXN_PAGE_CORES),
    });
  await send("Page.navigate", { url: `http://localhost:${webPort}/` });

  const timeout = Number(process.env.HXN_PAGE_TIMEOUT ?? 300) * 1000;
  const deadline = Date.now() + timeout;
  let title = "";
  while (Date.now() < deadline) {
    const state = await send("Runtime.evaluate", { expression: "document.title", returnByValue: true });
    title = state.result?.result?.value ?? "";
    if (/ — (done|trapped)$/.test(title)) break;
    await sleep(250);
  }
  // What the last console calls posted before the title changed.
  await sleep(250);
  written.sort((a, b) => a.timestamp - b.timestamp);
  // Ash's runtime writes its own notes to stdout, prefixed `[ash] `; they go
  // to stderr, as under `ash run`.
  for (const { error, text } of written) {
    if (!error && !text.startsWith("[ash] ")) out(text);
    else err(text);
  }
  if (written.length === 0)
    err(`no console output from the web page: ${attached.length} targets followed (${attached.join(", ") || "none"}), title "${title}"`);
  if (/ — done$/.test(title)) {
    // The title says done however the program ended; the page's log, which it
    // keeps in the document whether or not it shows it, has the exit status.
    const log = await send("Runtime.evaluate", {
      expression: `document.getElementById("out")?.textContent ?? ""`,
      returnByValue: true,
    });
    const exited = [...(log.result?.result?.value ?? "").matchAll(/exited with (-?\d+)/g)].pop();
    const code = exited ? Number(exited[1]) : 0;
    if (code === 0) status = 0;
    else err(`the program exited with ${code}`);
  } else if (!/ — trapped$/.test(title))
    err(`the web page did not finish in ${timeout / 1000} s (HXN_PAGE_TIMEOUT)`);
} catch (e) {
  err(e.message);
} finally {
  cleanup();
}
// Writes to a pipe are asynchronous on macOS, and exiting before they finish
// loses them, so each write is awaited. Each run of lines to one stream is
// written whole, so the two streams stay in the order the page wrote them.
const flush = (stream, text) => new Promise((resolve) => stream.write(text, resolve));
for (let i = 0; i < lines.length; ) {
  const error = lines[i].error;
  let text = "";
  for (; i < lines.length && lines[i].error === error; i++) text += lines[i].text + "\n";
  await flush(error ? process.stderr : process.stdout, text);
}
process.exit(status);
