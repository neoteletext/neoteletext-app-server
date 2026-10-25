import assert from "node:assert/strict";
import { test } from "node:test";

import { ANY_SUBCODE, type PageLink, packedLinksLine, readPackedLinks } from "../src/packed-links.ts";
import { readTti } from "../src/read-tti.ts";
import { decimalSubcode } from "../src/screen.ts";

/** A null link's six nibbles as packed characters: page FF, sub-code 3F7F (EN 300 706 clause 9.6.1). */
const NULL_LINK = "OOOGOC";
/** The link control byte with row 24 displayed and bits 1 to 3 set (Table 14), then two checksum characters. */
const ROW_24_SHOWN = "O@@";

test("a link to a sub-page is its page's digits and the sub-code's, in packed characters (EN 300 706 Figure 12)", () => {
  const links: (PageLink | null)[] = [{ page: 0x695, subcode: decimalSubcode(1) }, null, null, null, null, null];
  // Designation 0, then page units 5, tens 9, S1 1 and S2, S3, S4 0.
  assert.equal(packedLinksLine(0x601, links, true), `OL,27,@EIA@@@${NULL_LINK.repeat(5)}${ROW_24_SHOWN}`);
});

test("a link to another magazine sends the magazine bits that differ, and any sub-page is 3F7F", () => {
  // From magazine 6 (110) to 1 (001) every bit differs: M1 is S2's bit 4,
  // M2 and M3 are S4's bits 3 and 4 (Figure 12).
  const line = packedLinksLine(0x601, [null, null, null, null, null, { page: 0x100, subcode: null }], false);
  assert.equal(line, `OL,27,@${NULL_LINK.repeat(5)}@@OOOOG@@`);
  // Magazine 8 is sent as 0, so from magazine 8 to 8 nothing differs.
  assert.equal(packedLinksLine(0x800, [{ page: 0x8fe, subcode: null }, null, null, null, null, null], true).slice(0, 13), "OL,27,@NOOGOC");
});

test("packed links read back as written, with their sub-codes and the row 24 flag", () => {
  const links: (PageLink | null)[] = [
    { page: 0x630, subcode: null },
    { page: 0x602, subcode: null },
    { page: 0x600, subcode: null },
    { page: 0x695, subcode: decimalSubcode(40) },
    null,
    { page: 0x100, subcode: null },
  ];
  for (const shown of [true, false]) {
    const text = packedLinksLine(0x601, links, shown).slice("OL,27,".length);
    assert.deepEqual(readPackedLinks(0x601, text), { links, displayRow24: shown });
  }
  assert.equal(readPackedLinks(0x601, "D".padEnd(40, "@")), undefined, "X/27/4 is not the Fastext links");
  assert.equal(ANY_SUBCODE, 0x3f7f);
});

test("a TTI page's packed links are its links, the sub-codes kept, an FL line's naming none", () => {
  const line = packedLinksLine(0x601, [{ page: 0x695, subcode: decimalSubcode(2) }, null, null, null, null, { page: 0x100, subcode: null }], true);
  const [packed] = readTti(`PN,60101\nSC,0001\n${line}\nOL,1,picture\n`);
  assert.deepEqual(packed?.links, [0x695, null, null, null, null, 0x100]);
  assert.deepEqual(packed?.linkSubcodes, [0x0002, null, null, null, null, null]);
  assert.equal(packed?.fastextRow, true);
  const [hidden] = readTti(`PN,60101\n${packedLinksLine(0x601, [null, null, null, null, null, null], false)}\n`);
  assert.equal(hidden?.fastextRow, false, "a packed line may hide row 24, as FL never does");
  const [plain] = readTti("PN,60101\nFL,602,8FF,8FF,8FF,8FF,100\n");
  assert.deepEqual(plain?.linkSubcodes, [null, null, null, null, null, null]);
});
