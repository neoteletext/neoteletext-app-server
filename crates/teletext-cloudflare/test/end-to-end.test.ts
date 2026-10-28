// The Cloudflare host end to end (requirements 1.2, 1.5, 5.2 and 5.5):
// `wrangler dev` locally, or the deployed Worker when NEOTELETEXT_URL names
// it, with the kit's fixture channel's program connected through the bridge
// (scripts/bridge.ts). Run by `npm test` in this folder.
//
//   npm test                                            locally, a fresh state
//   NEOTELETEXT_URL=https://cf.neoteletext.com NEOTELETEXT_CHANNEL=demo \
//     NEOTELETEXT_PROGRAM_SECRET=... npm test           against the deployment

import { strict as assert } from "node:assert";
import { type ChildProcess, spawn } from "node:child_process";
import { rmSync } from "node:fs";
import { connect as connectPlain, type Socket } from "node:net";
import { dirname, join } from "node:path";
import { after, before, test } from "node:test";
import { connect as connectSecure } from "node:tls";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const crate = join(here, "..");
const repository = join(crate, "..", "..");
const fixture = join(repository, "crates", "teletext-native", "tests", "fixtures", "kit-channel.jsonl");
/** A local port for `wrangler dev`, apart from its default. */
const LOCAL_PORT = 8797;
/** The secret `.dev.vars` gives the local channel's program. */
const LOCAL_SECRET = "local-test-secret";
/** How long anything has to happen. */
const WITHIN_MS = 60_000;
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
/** How long a viewer on pages that do not change is watched for anything sent unasked. */
const QUIET_MS = 5_000;

const deployed = process.env.NEOTELETEXT_URL;
const base = deployed ?? `http://127.0.0.1:${LOCAL_PORT}`;
const channel = process.env.NEOTELETEXT_CHANNEL ?? "demo";
const secret = process.env.NEOTELETEXT_PROGRAM_SECRET ?? LOCAL_SECRET;

const running: ChildProcess[] = [];

function start(command: string, args: string[], options: { cwd: string; env?: NodeJS.ProcessEnv }): ChildProcess {
  const child = spawn(command, args, { cwd: options.cwd, env: { ...process.env, ...options.env }, stdio: ["ignore", "ignore", "inherit"] });
  running.push(child);
  return child;
}

async function stop(child: ChildProcess): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) return;
  const exited = new Promise((resolve) => child.once("exit", resolve));
  child.kill("SIGTERM");
  await exited;
}

function startWrangler(): ChildProcess {
  const tools = join(repository, "tools", "bin");
  return start("npx", ["wrangler", "dev", "--port", String(LOCAL_PORT), "--ip", "127.0.0.1", "--var", `ORIGIN:${base}`], {
    cwd: crate,
    env: { PATH: `${tools}:${process.env.PATH ?? ""}` },
  });
}

function startBridge(): ChildProcess {
  const program = 'head -n 1 "$0"; read -r _; tail -n +2 "$0"; exec cat >/dev/null';
  const url = `${base.replace(/^http/, "ws")}/${channel}/program`;
  return start("node", [join(repository, "scripts", "bridge.ts"), "--url", url, "--", "sh", "-c", program, fixture], {
    cwd: repository,
    env: { NEOTELETEXT_PROGRAM_SECRET: secret },
  });
}

/** Asks `check` until it answers, within [`WITHIN_MS`]. */
async function eventually<T>(check: () => Promise<T | undefined>, what: string): Promise<T> {
  const deadline = Date.now() + WITHIN_MS;
  for (;;) {
    const answer = await check().catch(() => undefined);
    if (answer !== undefined) return answer;
    assert.ok(Date.now() < deadline, `${what} did not happen within ${WITHIN_MS} ms`);
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
}

async function page(path: string): Promise<Buffer | undefined> {
  const response = await fetch(`${base}${path}`);
  return response.ok ? Buffer.from(await response.arrayBuffer()) : undefined;
}

/** A push connection opened by hand, to read the upgrade's headers and the server's frames. */
class Viewer {
  readonly socket: Socket;
  readonly head: string;
  #buffer: Buffer;

  private constructor(socket: Socket, head: string, rest: Buffer) {
    this.socket = socket;
    this.head = head;
    this.#buffer = rest;
  }

  static async open(cookie?: string, file = "push", headers: Record<string, string> = {}): Promise<Viewer> {
    const url = new URL(`${base}/${channel}/${file}`);
    const secure = url.protocol === "https:";
    const port = Number(url.port || (secure ? 443 : 80));
    const socket = secure ? connectSecure({ host: url.hostname, port, servername: url.hostname }) : connectPlain({ host: url.hostname, port });
    await new Promise((resolve, reject) => {
      socket.once(secure ? "secureConnect" : "connect", resolve);
      socket.once("error", reject);
    });
    const extra = Object.entries(cookie === undefined ? headers : { ...headers, Cookie: cookie })
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

  /** The session its `Set-Cookie` names, if it was given one. */
  get session(): string | undefined {
    const line = this.head.split("\r\n").find((header) => header.toLowerCase().startsWith("set-cookie:"));
    return line?.match(/_session=(\d+)\./)?.[1];
  }

  get cookie(): string {
    const line = this.head.split("\r\n").find((header) => header.toLowerCase().startsWith("set-cookie:")) ?? "";
    return line.slice(line.indexOf(":") + 1).trim().split(";")[0] ?? "";
  }

  /** The binary messages received so far, by their payloads' lengths. */
  messages(): number[] {
    return this.frames()
      .filter((frame) => frame.opcode === OPCODE_BINARY)
      .map((frame) => frame.length);
  }

  /** Sends a Ping of `payload`, masked with the key 0. */
  ping(payload: Buffer): void {
    this.socket.write(Buffer.concat([Buffer.from([FIN | OPCODE_PING, MASKED | payload.length, 0, 0, 0, 0]), payload]));
  }

  /** The frames received so far: each one's opcode, payload length and payload. */
  frames(): { opcode: number; length: number; payload: Buffer }[] {
    const frames: { opcode: number; length: number; payload: Buffer }[] = [];
    let at = 0;
    while (at + 2 <= this.#buffer.length) {
      let length = this.#buffer[at + 1]! & 0x7f;
      let header = 2;
      if (length === 126) {
        if (at + 4 > this.#buffer.length) break;
        length = this.#buffer.readUInt16BE(at + 2);
        header = 4;
      } else if (length === 127) {
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

let wrangler: ChildProcess | undefined;
let bridge: ChildProcess | undefined;

before(async () => {
  if (deployed === undefined) {
    rmSync(join(crate, ".wrangler", "state"), { recursive: true, force: true });
    wrangler = startWrangler();
    await eventually(async () => ((await fetch(`${base}/`)).ok ? true : undefined), "wrangler dev answering");
  }
  bridge = startBridge();
});

after(async () => {
  for (const child of running) await stop(child);
});

test("the program's pages are served, its section and new title listed", async () => {
  const index = await eventually(() => page(`/${channel}/100.t42`), "page 100");
  assert.equal(index.length % PACKET_BYTES, 0);
  // A deployment may serve page 100 kept from before (requirement 5.2)
  // while the program is still declaring and retitling: wait for its title.
  const contents = await eventually(async () => {
    const text = (await page(`/${channel}/contents.txt`))?.toString("utf8");
    return text?.includes("p\t102\tKit raw, retitled\n") ? text : undefined;
  }, "the program's new title listed");
  assert.match(contents, /p\t100\tKit index\n/);
  assert.match(contents, /p\t102\tKit raw, retitled\n/);
  assert.match(contents, /s\tKit links\np\t103\tKit sub-page link\n/);
});

test("a program without the channel's secret is refused, and a second is refused", async () => {
  const wrong = await Viewer.open(undefined, "program", { Authorization: "Bearer not-the-secret" });
  assert.equal(wrong.status, 401, wrong.head);
  wrong.close();
  await eventually(() => page(`/${channel}/100.t42`), "the first program connected");
  const second = await Viewer.open(undefined, "program", { Authorization: `Bearer ${secret}` });
  assert.equal(second.status, 409, second.head);
  second.close();
});

test("a viewer is sent the pages and nothing unasked after, its ping is answered, and it continues its session with its cookie", async () => {
  await eventually(() => page(`/${channel}/100.t42`), "page 100");
  const first = await Viewer.open();
  assert.equal(first.status, 101, first.head);
  const session = first.session;
  assert.ok(session !== undefined, `a cookie: ${first.head}`);
  assert.match(first.head.toLowerCase(), /set-cookie: _session=\d+\.\d+\.[0-9a-f]{64}; path=\/[^/]+\/; max-age=2592000; httponly; samesite=strict/);
  // Every shared page, and then, on pages that do not change, nothing: no
  // time-filling header (requirement 11.2).
  await eventually(async () => (first.messages().some((length) => length > PACKET_BYTES) ? true : undefined), "the pages themselves");
  await new Promise((resolve) => setTimeout(resolve, QUIET_MS));
  const sent = first.messages().length;
  await new Promise((resolve) => setTimeout(resolve, QUIET_MS));
  assert.equal(first.messages().length, sent, "nothing sent unasked");
  assert.ok(!first.messages().includes(PACKET_BYTES), "no header alone");
  // The keepalive: the runtime answers a ping (requirement 11.3).
  const payload = Buffer.from("keep");
  first.ping(payload);
  const pong = await eventually(async () => first.frames().find((frame) => frame.opcode === OPCODE_PONG), "a pong");
  assert.deepEqual(pong.payload, payload, "the ping's payload");
  const cookie = first.cookie;
  first.close();
  const again = await eventually(async () => {
    const viewer = await Viewer.open(cookie);
    if (viewer.session === undefined) {
      // The first connection's close has not reached the object yet.
      viewer.close();
      return undefined;
    }
    return viewer;
  }, "the session continued");
  assert.equal(again.session, session, "the same session");
  again.close();
});

test("a restarted host serves what it kept before the program connects again", { skip: deployed !== undefined && "local only" }, async () => {
  const kept = await eventually(() => page(`/${channel}/100.t42`), "page 100");
  await stop(bridge!);
  await stop(wrangler!);
  wrangler = startWrangler();
  const served = await eventually(() => page(`/${channel}/100.t42`), "page 100 after the restart");
  assert.deepEqual(served, kept, "the page as it was kept");
});
