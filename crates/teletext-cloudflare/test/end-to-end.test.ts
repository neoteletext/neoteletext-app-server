// The Cloudflare host end to end (REQ-1021, REQ-1024, REQ-1674; TODO-2054
// item 4, TODO-4435, TODO-4579): `wrangler dev` locally, or the deployed
// Worker when NEOTELETEXT_URL names it, with the TypeScript kit's fixture
// channel (test/fixtures/hello.ts) connected as the channel's program
// through the bridge (scripts/bridge.ts). Run by `npm test` in this folder.
//
//   npm test                                            locally, a fresh state
//   NEOTELETEXT_URL=https://cf.neoteletext.com NEOTELETEXT_CHANNEL=demo \
//     NEOTELETEXT_PROGRAM_SECRET=... npm test           against the deployment
//
// It needs the kit's checkout with its generated validators current
// (NEOTELETEXT_KIT, by default ~/devel/neoteletext-server-ts-protocol;
// `node scripts/validators.ts` there) and the engine's `neottx` at the
// server's pinned engine commit (NEOTTX); either missing fails the run.

import { strict as assert } from "node:assert";
import { type ChildProcess, spawn, spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { homedir } from "node:os";
import { connect as connectPlain, type Socket } from "node:net";
import { dirname, join } from "node:path";
import { after, before, test } from "node:test";
import { connect as connectSecure } from "node:tls";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const crate = join(here, "..");
const repository = join(crate, "..", "..");
/** A local port for `wrangler dev`, apart from its default. */
const LOCAL_PORT = 8797;
/** The secret `.dev.vars` gives the local channel's program. */
const LOCAL_SECRET = "local-test-secret";
/** How long anything has to happen. */
const WITHIN_MS = 60_000;
/** How long `wrangler dev` has to answer: it first builds the Worker, which took 68 s on chunko on 21 November 2026; four times that. */
const WRANGLER_STARTS_WITHIN_MS = 300_000;
/** A T42 packet's bytes (EN 300 706 clause 7.1). */
const PACKET_BYTES = 42;
/** WebSocket opcodes (RFC 6455 section 5.2). */
const OPCODE_BINARY = 0x2;
const OPCODE_PING = 0x9;
const OPCODE_PONG = 0xa;
/** A frame's first byte: the final fragment. */
const FIN = 0x80;
/** A client frame's second byte: masked, as every client frame is (RFC 6455 section 5.3). */
const MASKED = 0x80;
/** A payload length that needs the 16-bit extended length (RFC 6455 section 5.2). */
const LENGTH_16 = 126;
const LENGTH_64 = 127;
/** How long a viewer on pages that do not change is watched for anything sent unasked. */
const QUIET_MS = 5_000;
/**
 * How long a viewer and its program stay quiet for the object to hibernate:
 * Cloudflare evicts an object that "receives no events ... for a short
 * period" (its Durable Objects documentation, "Use WebSockets") and states
 * no figure. The test does not take the wait on trust: a wake shows as the
 * program's second `start`, which it waits for.
 */
const HIBERNATE_WAIT_MS = 30_000;
/** The input protocol (neoteletext-input's `event.rs`): an event item's lead byte, HELLO and SELECT. */
const EVENT = 0xff;
const HELLO = 0x09;
const SELECT = 0x40;
/** The input protocol's version (neoteletext-input `limits::VERSION`), which push protocol 10 names. */
const INPUT_VERSION = 10;
/** SELECT_CAP (neoteletext-input `caps.rs`): the receiver selects pages. */
const SELECT_CAP = 1 << 22;
/** The push WebSocket's subprotocol (neoteletext-web `PUSH_SUBPROTOCOL`, DOC-1917). */
const SUBPROTOCOL = "neottx.10";
/** A LEB128 group's bits and its continuation bit (neoteletext-input `wire.rs`). */
const GROUP_BITS = 7;
const GROUP = 0x7f;
const MORE = 0x80;
/** The kit fixture's carousel and its second sub-page, which the program sends again after a wake only if the selection was kept. */
const CAROUSEL_PLACE = "101/0002";

const deployed = process.env.NEOTELETEXT_URL;
const base = deployed ?? `http://127.0.0.1:${LOCAL_PORT}`;
const channel = process.env.NEOTELETEXT_CHANNEL ?? "demo";
const secret = process.env.NEOTELETEXT_PROGRAM_SECRET ?? LOCAL_SECRET;
const kit = process.env.NEOTELETEXT_KIT ?? join(homedir(), "devel", "neoteletext-server-ts-protocol");
const neottx = process.env.NEOTTX ?? join(repository, "tmp", "neottx", "bin", "neottx");
const scratch = mkdtempSync(join(process.env.TMPDIR ?? join(repository, "tmp"), "cloudflare-end-to-end-"));
/** Every line the server sent the program, as the program read them. */
const programInput = join(scratch, "program-input.jsonl");

const running: ChildProcess[] = [];

function start(command: string, args: string[], options: { cwd: string; env?: NodeJS.ProcessEnv }): ChildProcess {
  // Each in a process group of its own, so that stopping it stops what it
  // started (wrangler's workerd, the bridge's program).
  const child = spawn(command, args, { cwd: options.cwd, env: { ...process.env, ...options.env }, stdio: ["ignore", "ignore", "inherit"], detached: true });
  running.push(child);
  return child;
}

async function stop(child: ChildProcess): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) return;
  const exited = new Promise((resolve) => child.once("exit", resolve));
  if (child.pid !== undefined) process.kill(-child.pid, "SIGTERM");
  await exited;
}

function startWrangler(): ChildProcess {
  const tools = join(repository, "tools");
  return start(join(crate, "node_modules", ".bin", "wrangler"), ["dev", "--port", String(LOCAL_PORT), "--ip", "127.0.0.1", "--var", `ORIGIN:${base}`], {
    cwd: crate,
    env: { PATH: `${join(tools, "bin")}:${process.env.PATH ?? ""}`, XDG_CACHE_HOME: join(tools, "cache"), WRANGLER_SEND_METRICS: "false" },
  });
}

/** The kit's fixture channel behind the bridge, every line it is sent kept in `programInput`. */
function startBridge(): ChildProcess {
  const program = 'tee -a "$0" | exec node test/fixtures/hello.ts';
  const url = `${base.replace(/^http/, "ws")}/${channel}/program`;
  return start("node", [join(repository, "scripts", "bridge.ts"), "--url", url, "--directory", kit, "--", "sh", "-c", program, programInput], {
    cwd: repository,
    env: { NEOTELETEXT_PROGRAM_SECRET: secret },
  });
}

/** The lines the program has been sent so far. */
function programLines(): Record<string, unknown>[] {
  if (!existsSync(programInput)) return [];
  return readFileSync(programInput, "utf8")
    .split("\n")
    .filter((line) => line !== "")
    .map((line) => JSON.parse(line) as Record<string, unknown>);
}

/** Asks `check` until it answers, within `within` ms. */
async function eventually<T>(check: () => Promise<T | undefined>, what: string, within = WITHIN_MS): Promise<T> {
  const deadline = Date.now() + within;
  for (;;) {
    const answer = await check().catch(() => undefined);
    if (answer !== undefined) return answer;
    assert.ok(Date.now() < deadline, `${what} did not happen within ${within} ms`);
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
}

function pause(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/** An unsigned LEB128 varint (neoteletext-input `Writer::uv`). */
function varint(value: number): number[] {
  const bytes: number[] = [];
  let rest = value;
  for (;;) {
    const low = rest & GROUP;
    rest = Math.floor(rest / (1 << GROUP_BITS));
    if (rest === 0) return [...bytes, low];
    bytes.push(low | MORE);
  }
}

/** HELLO of a receiver that selects pages and nothing else. */
function hello(): Buffer {
  return Buffer.from([EVENT, HELLO, ...varint(INPUT_VERSION), ...varint(SELECT_CAP)]);
}

/** SELECT of a place such as `101/0002`, with no query. */
function select(place: string): Buffer {
  return Buffer.from([EVENT, SELECT, ...varint(place.length), ...Buffer.from(place, "latin1")]);
}

/** A push or program connection opened by hand, to read the upgrade's headers and the server's frames. */
class Viewer {
  readonly socket: Socket;
  readonly head: string;
  #buffer: Buffer;

  private constructor(socket: Socket, head: string, rest: Buffer) {
    this.socket = socket;
    this.head = head;
    this.#buffer = rest;
  }

  static async open(file = "push", headers: Record<string, string> = { "Sec-WebSocket-Protocol": SUBPROTOCOL }): Promise<Viewer> {
    const url = new URL(`${base}/${channel}/${file}`);
    const secure = url.protocol === "https:";
    const port = Number(url.port || (secure ? 443 : 80));
    const socket = secure ? connectSecure({ host: url.hostname, port, servername: url.hostname, ALPNProtocols: ["http/1.1"] }) : connectPlain({ host: url.hostname, port });
    await new Promise((resolve, reject) => {
      socket.once(secure ? "secureConnect" : "connect", resolve);
      socket.once("error", reject);
    });
    const extra = Object.entries(headers)
      .map(([name, value]) => `${name}: ${value}\r\n`)
      .join("");
    socket.write(
      `GET ${url.pathname} HTTP/1.1\r\nHost: ${url.host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n${extra}\r\n`,
    );
    let received = Buffer.alloc(0);
    for (;;) {
      const chunk: Buffer = await new Promise((resolve, reject) => {
        socket.once("data", resolve);
        socket.once("error", reject);
      });
      received = Buffer.concat([received, chunk]);
      const end = received.indexOf("\r\n\r\n");
      if (end !== -1) {
        const viewer = new Viewer(socket, received.subarray(0, end).toString("latin1"), received.subarray(end + 4));
        socket.on("data", (more: Buffer) => {
          viewer.#buffer = Buffer.concat([viewer.#buffer, more]);
        });
        return viewer;
      }
    }
  }

  get status(): number {
    return Number(this.head.split(" ")[1]);
  }

  /** A response header's value, or undefined. */
  header(name: string): string | undefined {
    const line = this.head.split("\r\n").find((header) => header.toLowerCase().startsWith(`${name.toLowerCase()}:`));
    return line?.slice(name.length + 1).trim();
  }

  /** The binary messages received so far, by their payloads' lengths. */
  messages(): number[] {
    return this.frames()
      .filter((frame) => frame.opcode === OPCODE_BINARY)
      .map((frame) => frame.length);
  }

  /** Sends one binary message, masked with the key 0. */
  send(payload: Buffer): void {
    this.#frame(OPCODE_BINARY, payload);
  }

  /** Sends a Ping of `payload`, masked with the key 0. */
  ping(payload: Buffer): void {
    this.#frame(OPCODE_PING, payload);
  }

  #frame(opcode: number, payload: Buffer): void {
    assert.ok(payload.length < LENGTH_16, "a short frame");
    this.socket.write(Buffer.concat([Buffer.from([FIN | opcode, MASKED | payload.length, 0, 0, 0, 0]), payload]));
  }

  /** The frames received so far: each one's opcode, payload length and payload. */
  frames(): { opcode: number; length: number; payload: Buffer }[] {
    const frames: { opcode: number; length: number; payload: Buffer }[] = [];
    let at = 0;
    while (at + 2 <= this.#buffer.length) {
      let length = this.#buffer[at + 1]! & 0x7f;
      let header = 2;
      if (length === LENGTH_16) {
        if (at + 4 > this.#buffer.length) break;
        length = this.#buffer.readUInt16BE(at + 2);
        header = 4;
      } else if (length === LENGTH_64) {
        if (at + 10 > this.#buffer.length) break;
        length = Number(this.#buffer.readBigUInt64BE(at + 2));
        header = 10;
      }
      if (at + header + length > this.#buffer.length) break;
      frames.push({ opcode: this.#buffer[at]! & 0x0f, length, payload: this.#buffer.subarray(at + header, at + header + length) });
      at += header + length;
    }
    return frames;
  }

  close(): void {
    this.socket.destroy();
  }
}

/** Whether the program has been sent `connected` for a selection of `place`, after the `start` numbered `starts`. */
function connectedAfterStart(starts: number, place: string): boolean {
  let seen = 0;
  for (const line of programLines()) {
    if (line.type === "start") seen += 1;
    else if (seen >= starts && line.type === "connected" && line.path === place) return true;
  }
  return false;
}

function starts(): number {
  return programLines().filter((line) => line.type === "start").length;
}

before(async () => {
  assert.ok(existsSync(join(kit, "test", "fixtures", "hello.ts")), `the kit's fixture channel is needed at ${kit} (NEOTELETEXT_KIT)`);
  assert.ok(existsSync(neottx), `neottx at the server's pinned engine commit is needed at ${neottx} (NEOTTX)`);
  if (deployed === undefined) {
    rmSync(join(crate, ".wrangler", "state"), { recursive: true, force: true });
    startWrangler();
    await eventually(async () => ((await fetch(`${base}/${channel}/`)).ok ? true : undefined), "wrangler dev answering", WRANGLER_STARTS_WITHIN_MS);
  }
  startBridge();
  await eventually(async () => (starts() > 0 ? true : undefined), "the program sent start");
});

after(async () => {
  for (const child of running) await stop(child);
  rmSync(scratch, { recursive: true, force: true });
});

test("the push upgrade is refused as the native host refuses it, and accepted naming the subprotocol (TODO-4435)", async () => {
  const withOrigin = await Viewer.open("push", { Origin: "https://elsewhere.example", "Sec-WebSocket-Protocol": SUBPROTOCOL });
  assert.equal(withOrigin.status, 403, withOrigin.head);
  withOrigin.close();
  const without = await Viewer.open("push", {});
  assert.equal(without.status, 426, without.head);
  assert.equal(without.header("sec-websocket-protocol"), SUBPROTOCOL, without.head);
  without.close();
  const other = await Viewer.open("push", { "Sec-WebSocket-Protocol": "neottx.9" });
  assert.equal(other.status, 426, other.head);
  other.close();
  const accepted = await Viewer.open();
  assert.equal(accepted.status, 101, accepted.head);
  assert.equal(accepted.header("sec-websocket-protocol"), SUBPROTOCOL, accepted.head);
  // The server keeps no session and sets no cookie (REQ-6704, DEC-6471).
  assert.equal(accepted.header("set-cookie"), undefined, accepted.head);
  accepted.close();
});

test("a program without the channel's secret is refused 401, and a second 409 (REQ-1024)", async () => {
  const none = await Viewer.open("program", {});
  assert.equal(none.status, 401, none.head);
  none.close();
  const wrong = await Viewer.open("program", { Authorization: "Bearer not-the-secret" });
  assert.equal(wrong.status, 401, wrong.head);
  wrong.close();
  const second = await Viewer.open("program", { Authorization: `Bearer ${secret}` });
  assert.equal(second.status, 409, second.head);
  second.close();
});

test("the channel's addresses answer as the URL map says (REQ-1693 item 7)", async () => {
  const get = (path: string) => fetch(`${base}${path}`, { redirect: "manual" });
  assert.equal((await get("/")).status, 404);
  const folder = await get(`/${channel}`);
  assert.equal(folder.status, 308);
  assert.equal(folder.headers.get("location"), `/${channel}/`);
  const small = await get(`/${channel}/`);
  assert.equal(small.status, 200);
  assert.match(small.headers.get("content-type") ?? "", /^text\/html/);
  assert.ok(small.headers.get("content-security-policy"), "a policy");
  const descriptor = await get(`/${channel}/channel.ttx`);
  assert.equal(descriptor.status, 200);
  assert.equal(typeof ((await descriptor.json()) as { name?: unknown }).name, "string");
  const page = await get(`/${channel}/100`);
  assert.equal(page.status, 200, "a page's address answers, public or not (REQ-1728)");
  assert.equal((await get(`/${channel}/100/`)).status, 308);
  assert.equal((await get(`/${channel}/nothing-here`)).status, 404);
});

test("a receiver meets the channel as neottx probe does, and is served the program's page", () => {
  const local = deployed === undefined ? ["--allow-local", "127.0.0.1"] : [];
  const probe = spawnSync(neottx, ["probe", ...local, `${base}/${channel}/`], { encoding: "utf8", timeout: WITHIN_MS });
  assert.equal(probe.status, 0, `${probe.stdout}\n${probe.stderr}`);
  assert.match(probe.stdout, /handshake +accepted: neottx\.10/);
  assert.match(probe.stdout, /selection +100: a page/);
});

test("a viewer is sent its page and nothing unasked after, and its ping is answered (requirements 11.2, 11.3)", async () => {
  const viewer = await Viewer.open();
  assert.equal(viewer.status, 101, viewer.head);
  viewer.send(hello());
  viewer.send(select("100"));
  await eventually(async () => (viewer.messages().some((length) => length > PACKET_BYTES) ? true : undefined), "the page itself");
  await pause(QUIET_MS);
  const sent = viewer.messages().length;
  await pause(QUIET_MS);
  assert.equal(viewer.messages().length, sent, "nothing sent unasked");
  const payload = Buffer.from("keep");
  viewer.ping(payload);
  const pong = await eventually(async () => viewer.frames().find((frame) => frame.opcode === OPCODE_PONG), "a pong");
  assert.deepEqual(pong.payload, payload, "the ping's payload");
  viewer.close();
});

test(
  "a viewer's selection outlasts the object's hibernation (DEC-1755 option 1, TODO-4579)",
  { skip: deployed === undefined && "wrangler dev keeps the object in memory, so a hibernation cannot be forced: run against the deployment" },
  async () => {
    const viewer = await Viewer.open();
    assert.equal(viewer.status, 101, viewer.head);
    viewer.send(hello());
    viewer.send(select(CAROUSEL_PLACE));
    await eventually(async () => (connectedAfterStart(starts(), CAROUSEL_PLACE) || viewer.messages().length > 0 ? true : undefined), "the selection");
    await eventually(async () => (viewer.messages().some((length) => length > PACKET_BYTES) ? true : undefined), "the sub-page itself");
    const before = { starts: starts(), messages: viewer.messages().length };
    // Quiet on both sides: the object hibernates. A request wakes it, which
    // gives the viewer's connection back and starts the program again.
    await pause(HIBERNATE_WAIT_MS);
    assert.equal((await fetch(`${base}/${channel}/channel.ttx`)).status, 200);
    await eventually(async () => (starts() > before.starts ? true : undefined), "a wake: the program sent start again");
    await eventually(
      async () => (connectedAfterStart(before.starts + 1, CAROUSEL_PLACE) ? true : undefined),
      `the program told the connection with its selection ${CAROUSEL_PLACE} after the wake`,
    );
    await eventually(async () => (viewer.messages().length > before.messages ? true : undefined), "the sub-page sent again after the wake");
    viewer.close();
  },
);
