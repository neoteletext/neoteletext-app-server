// The bridge between a channel's program and the Cloudflare host (decision
// 20): runs the program over its standard input and output, as the native
// server does (docs/applications.md), and carries its lines to and from its
// channel's Durable Object over a WebSocket, one line a message
// (requirements 1.5 and 6.4). The program is unchanged.
//
//   NEOTELETEXT_PROGRAM_SECRET=... node scripts/bridge.ts \
//       --url wss://cf.neoteletext.com/demosite/program [--directory DIR] -- node src/main.ts
//
// When the connection closes it connects again after a pause that doubles,
// up to 30 s, and declares the program's pages again; the channel then
// sends `start` afresh. When the program ends it is started again the same
// way, on a new connection, so the channel sees a program that started.
// Node 22.4 or later: the WebSocket and the type stripping are built in.

import { spawn, type ChildProcess } from "node:child_process";
import { createInterface } from "node:readline";

/** The variable holding the channel's secret, never a flag: flags show in `ps`. */
const SECRET_VARIABLE = "NEOTELETEXT_PROGRAM_SECRET";
/** The first pause before connecting or starting again, in milliseconds. */
const FIRST_PAUSE_MS = 1_000;
/** The longest pause (docs/applications.md section 1). */
const LONGEST_PAUSE_MS = 30_000;
/** Lines the program sends before the connection opens that are kept for it. */
const WAITING_LINES = 4_096;

interface Options {
  url: string;
  directory: string;
  command: string[];
}

function options(argv: string[]): Options {
  const split = argv.indexOf("--");
  const flags = split === -1 ? argv : argv.slice(0, split);
  const command = split === -1 ? [] : argv.slice(split + 1);
  let url = "";
  let directory = process.cwd();
  for (let index = 0; index < flags.length; index += 2) {
    const value = flags[index + 1] ?? "";
    if (flags[index] === "--url") url = value;
    else if (flags[index] === "--directory") directory = value;
    else throw new Error(`unknown argument ${flags[index]}`);
  }
  if (url === "" || command.length === 0) {
    throw new Error("usage: node scripts/bridge.ts --url wss://HOST/CHANNEL/program [--directory DIR] -- PROGRAM [ARGUMENTS]");
  }
  return { url, directory, command };
}

function say(text: string): void {
  process.stderr.write(`bridge: ${text}\n`);
}

class Bridge {
  readonly #options: Options;
  readonly #secret: string;
  #program: ChildProcess | undefined;
  #socket: WebSocket | undefined;
  #declaration: string | undefined;
  #waiting: string[] = [];
  #connectPause = FIRST_PAUSE_MS;
  #programPause = FIRST_PAUSE_MS;
  #stopping = false;

  constructor(options: Options, secret: string) {
    this.#options = options;
    this.#secret = secret;
  }

  start(): void {
    this.#startProgram();
  }

  stop(): void {
    this.#stopping = true;
    this.#socket?.close();
    this.#program?.kill();
  }

  #startProgram(): void {
    const [program = "", ...rest] = this.#options.command;
    const child = spawn(program, rest, { cwd: this.#options.directory, stdio: ["pipe", "pipe", "inherit"] });
    this.#program = child;
    this.#declaration = undefined;
    this.#waiting = [];
    const startedAt = Date.now();
    createInterface({ input: child.stdout! }).on("line", (line) => this.#fromProgram(line));
    child.on("exit", (code, signal) => {
      if (this.#stopping) return;
      // A program that ran steadily was ended on purpose: at once.
      const steady = Date.now() - startedAt >= 5_000;
      const pause = steady ? 0 : this.#programPause;
      this.#programPause = steady ? FIRST_PAUSE_MS : Math.min(this.#programPause * 2, LONGEST_PAUSE_MS);
      say(`the program ended (${signal ?? code}); starting it again in ${pause} ms`);
      // The channel sees a program that started: a new connection.
      this.#socket?.close();
      setTimeout(() => this.#startProgram(), pause);
    });
    child.on("error", (error) => say(`cannot start ${program}: ${error.message}`));
  }

  #fromProgram(line: string): void {
    if (line.trim() === "") return;
    if (this.#declaration === undefined) {
      this.#declaration = line;
      this.#connect();
      return;
    }
    if (this.#socket?.readyState === WebSocket.OPEN) {
      this.#socket.send(line);
    } else if (this.#waiting.length < WAITING_LINES) {
      this.#waiting.push(line);
    }
  }

  #connect(): void {
    const declaration = this.#declaration;
    if (declaration === undefined || this.#stopping) return;
    // Node's WebSocket takes headers in its second argument, beyond the
    // web's interface; the secret goes in one, never in the address.
    const init = { headers: { authorization: `Bearer ${this.#secret}` } };
    const socket = new WebSocket(this.#options.url, init as unknown as string[]);
    this.#socket = socket;
    socket.addEventListener("open", () => {
      say(`connected to ${this.#options.url}`);
      this.#connectPause = FIRST_PAUSE_MS;
      socket.send(declaration);
      for (const line of this.#waiting.splice(0)) socket.send(line);
    });
    socket.addEventListener("message", (event) => {
      const text = typeof event.data === "string" ? event.data : Buffer.from(event.data as ArrayBuffer).toString("utf8");
      this.#program?.stdin?.write(`${text}\n`);
    });
    socket.addEventListener("close", (event) => {
      if (this.#socket !== socket || this.#stopping) return;
      const pause = this.#connectPause;
      this.#connectPause = Math.min(this.#connectPause * 2, LONGEST_PAUSE_MS);
      say(`the connection closed (${event.code} ${event.reason}); connecting again in ${pause} ms`);
      this.#socket = undefined;
      setTimeout(() => this.#connect(), pause);
    });
    socket.addEventListener("error", () => {
      // The close that follows connects again.
    });
  }
}

const secret = process.env[SECRET_VARIABLE] ?? "";
if (secret === "") {
  say(`${SECRET_VARIABLE} must hold the channel's secret`);
  process.exit(1);
}
const bridge = new Bridge(options(process.argv.slice(2)), secret);
bridge.start();
for (const signal of ["SIGINT", "SIGTERM"] as const) {
  process.on(signal, () => {
    bridge.stop();
    process.exit(0);
  });
}
