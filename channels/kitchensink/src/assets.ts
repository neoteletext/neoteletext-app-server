// What the kitchensink takes from outside the program, read once at its
// start: the sounds it makes, the QR codes the engine's `neottx qr` writes
// (the engine's tool, run, not rewritten), and the converted pictures the
// showcase reads from the same folder (the showcase's loader, imported).

import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";

import { noise, sequence, sound, type Sound, tone } from "../../../kits/typescript-components/src/index.ts";
import { type Picture, loadPictures } from "../../showcase/src/pictures.ts";

export type { Picture };

/** The engine's command line program; `NEOTTX` names another. */
const NEOTTX = process.env["NEOTTX"] ?? join(homedir(), "devel/neoteletext-engine/target/release/neottx");
/** How long one code may take. */
const QR_WITHIN_MS = 10_000;

/** The QR code of each address, as TTI; an address the tool could not draw is left out, and the page says so. */
export function qrCodes(addresses: readonly string[]): Map<string, string> {
  const codes = new Map<string, string>();
  const folder = mkdtempSync(join(tmpdir(), "kitchensink-qr-"));
  try {
    addresses.forEach((address, index) => {
      const target = join(folder, `${index}.tti`);
      try {
        execFileSync(NEOTTX, ["qr", "--page", "100", address, target], { timeout: QR_WITHIN_MS, stdio: ["ignore", "ignore", "pipe"] });
        codes.set(address, readFileSync(target, "latin1"));
      } catch (error) {
        process.stderr.write(`kitchensink: no QR code for ${address}: ${String(error).slice(0, 200)}\n`);
      }
    });
  } finally {
    rmSync(folder, { recursive: true, force: true });
  }
  return codes;
}

/** The converted pictures in `folder`, none when it is not given. */
export function pictures(folder: string | undefined): readonly Picture[] {
  return folder === undefined ? [] : loadPictures(folder);
}

/** The sounds the pages play. */
export const SOUNDS = {
  click: sound("Click", tone(1_200, 30, "square", 0.4)),
  right: sound("Right", sequence(tone(660, 120), tone(880, 200))),
  wrong: sound("Wrong", sequence(tone(220, 180, "square", 0.5), tone(185, 260, "square", 0.5))),
  tick: sound("Tick", tone(1_800, 15, "square", 0.3)),
  done: sound("Ding", sequence(tone(1_046, 120), tone(1_318, 120), tone(1_568, 300))),
  eat: sound("Eat", tone(990, 60, "triangle")),
  crash: sound("Crash", noise(400, 0.6)),
  chime: sound("Chime", sequence(tone(784, 150), tone(659, 150), tone(523, 400))),
  scale: sound("Scale", sequence(...[262, 294, 330, 349, 392, 440, 494, 523].map((frequency) => tone(frequency, 140)))),
  siren: sound("Siren", sequence(...Array.from({ length: 6 }, (_, index) => tone(index % 2 === 0 ? 700 : 900, 200, "triangle")))),
} as const satisfies Record<string, Sound>;
