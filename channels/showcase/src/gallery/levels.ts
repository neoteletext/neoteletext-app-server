// A picture's three levels as the sub-pages of one page
// (crates/teletext-showcase/src/gallery/levels.rs): each level's TTI
// file, its first page, numbered as the gallery's page and given the
// gallery's links. The Rust read each file into the engine's page model
// and numbered that; here the file's text is rewritten, line for line,
// and the server reads it with the same engine reader.

import { type Level, LEVELS } from "../pictures.ts";
import { type PageLink, PACKED_LINKS, PACKED_LINKS_ROW, decimalSubcode, packedLinksLine, pageText } from "../kit.ts";

/** The sub-page each level is shown as: 1 Level 1.5, 2 Level 2.5, 3 Level 3.5 (crates/teletext-native's pictures.rs). */
const SUBPAGES: Readonly<Record<Level, number>> = { l15: 1, l25: 2, l35: 3 };
if (LEVELS.some((level, index) => SUBPAGES[level] !== index + 1)) {
  throw new Error("the levels are shown as sub-pages 1, 2 and 3, in order");
}

/** The sub-page a level is shown as. */
export function subpageOf(level: Level): number {
  return SUBPAGES[level];
}

/** A Fastext link to page `page` (such as 0x601), at sub-page `subpage` or any. */
export function link(page: number, subpage: number | null): PageLink {
  return { page, subcode: subpage === null ? null : decimalSubcode(subpage) };
}

/** The letters of a TTI command, which the engine's reader takes upper-cased, whatever follows them up to the first comma (its TTI_COMMAND_BYTES). */
const COMMAND_LETTERS = 2;
/** The line that starts a page in TTI, and the lines this rewrites. */
const PAGE_NUMBER = "PN";
const SUBCODE = "SC";
const FASTEXT = "FL";
const OUTPUT_LINE = "OL";
/** The description and source lines, which before a file's first PN line are every page's, and after it the page's (the engine's TTI reader). */
const DESCRIPTIONS = ["DE", "DS"] as const;
/**
 * The commands of a page (the engine's TTI reader, its PAGE_COMMANDS),
 * but for FH and TF, which it reports as withdrawn. Every other line, an
 * unknown one among them, is left out: the engine read past it while
 * reading the file, but the server refuses a publication that holds one.
 */
const PAGE_COMMANDS: ReadonlySet<string> = new Set(["SC", "PS", "OL", "FL", "LN", "LK", "LT", "RE", "PF", "CK", "FD", "OP", "FS", "FP", "FT", "CU", "IM"]);
/** The hexadecimal digits of a sub-code in TTI's SC line. */
const SUBCODE_DIGITS = 4;
/** The digits of a sub-page in a PN line, after the page's three. */
const SUBPAGE_DIGITS = 2;
/** TTI's FL value for no link (the engine's TTI_FL_NULL_LINK). */
const NO_LINK = "8FF";
/** The designation code of the Fastext links among X/27 packets (EN 300 706 clause 9.6.1), and the low four bits a packed character carries. */
const LINKS_DESIGNATION = 0;
const NIBBLE_MASK = 0x0f;
/** ESC and the escape form's offset of an OL line's control codes (the engine's TTI reader, REQ-INP-013). */
const ESCAPE = "\u001b";
const ESCAPE_OFFSET = 0x40;
const DECIMAL = 10;
const HEXADECIMAL = 16;

/** A TTI text's lines, each without its line ending. */
function lines(tti: string): string[] {
  return tti.split("\n").map((line) => line.replace(/\r+$/u, ""));
}

/** The command of a TTI line, upper-cased as the engine's reader takes it. */
function command(line: string): string {
  return line.slice(0, COMMAND_LETTERS).toUpperCase();
}

/** What follows a line's first comma. */
function argumentsOf(line: string): string {
  const comma = line.indexOf(",");
  return comma === -1 ? "" : line.slice(comma + 1);
}

function isPageCommand(line: string): boolean {
  return PAGE_COMMANDS.has(command(line));
}

/** Whether `line` names a description or a source. */
function isDescription(line: string): boolean {
  return DESCRIPTIONS.some((description) => command(line) === description);
}

/** Whether `line` is a packed X/27/0 line, which sets the page's Fastext links. */
function isFastextPacket(line: string): boolean {
  if (command(line) !== OUTPUT_LINE) {
    return false;
  }
  const rest = argumentsOf(line);
  const comma = rest.indexOf(",");
  if (comma === -1 || Number.parseInt(rest.slice(0, comma), DECIMAL) !== PACKED_LINKS_ROW) {
    return false;
  }
  const text = rest.slice(comma + 1);
  const code = text.startsWith(ESCAPE) ? text.charCodeAt(1) - ESCAPE_OFFSET : text.charCodeAt(0);
  return (code & NIBBLE_MASK) === LINKS_DESIGNATION;
}

/**
 * The first page of a TTI file, as the engine's reader finds it: its
 * lines after its PN line up to the next, and the file's description and
 * source lines before it, which are the page's too; undefined when the
 * file holds no page. Lines of a page before any PN line are a page of
 * their own, as the reader takes them, without the file's description.
 */
export function read(tti: string): { readonly head: readonly string[]; readonly page: readonly string[] } | undefined {
  const all = lines(tti);
  const first = all.findIndex((line) => command(line) === PAGE_NUMBER || isPageCommand(line));
  const opening = all[first];
  if (opening === undefined) {
    return undefined;
  }
  const numbered = command(opening) === PAGE_NUMBER;
  const start = numbered ? first + 1 : first;
  const next = all.findIndex((line, index) => index >= start && command(line) === PAGE_NUMBER);
  const body = all.slice(start, next === -1 ? all.length : next);
  return {
    head: numbered ? all.slice(0, first).filter(isDescription) : [],
    page: body.filter((line) => isPageCommand(line) || (numbered && isDescription(line))),
  };
}

/** The line of `links` for page `page`: FL where every link is to any sub-page, as the engine's TTI writer does, a packed X/27/0 line otherwise. */
function linksLine(page: number, links: readonly (PageLink | null)[]): string {
  if (links.every((target) => target === null || target.subcode === null)) {
    return `${FASTEXT},${links.map((target) => (target === null ? NO_LINK : pageText(target.page))).join(",")}`;
  }
  // The picture fills rows 0 to 24, so its row 24 stays, under the
  // gallery's links.
  return packedLinksLine(page, links, true);
}

/**
 * `levels`, each a sub-page and its TTI, as the sub-pages of page `page`
 * in one TTI text, with `fastext` (red, green, yellow, cyan, link 5 and
 * the index): a level that holds no page is left out, and none is
 * undefined. Each keeps its own lines but for its number, sub-code and
 * links; its description goes after its PN line, where it is the page's
 * as it was the file's.
 */
export function subpages(levels: readonly { readonly subpage: number; readonly tti: string }[], page: number, fastext: readonly (PageLink | null)[]): string | undefined {
  if (fastext.length !== PACKED_LINKS) {
    throw new RangeError(`a page has ${PACKED_LINKS} links`);
  }
  const written: string[] = [];
  for (const { subpage, tti } of levels) {
    const file = read(tti);
    if (file === undefined) {
      continue;
    }
    const kept = file.page.filter((line) => command(line) !== SUBCODE && command(line) !== FASTEXT && !isFastextPacket(line));
    written.push(
      `${PAGE_NUMBER},${pageText(page)}${subpage.toString().padStart(SUBPAGE_DIGITS, "0")}`,
      `${SUBCODE},${decimalSubcode(subpage).toString(HEXADECIMAL).toUpperCase().padStart(SUBCODE_DIGITS, "0")}`,
      ...file.head,
      ...kept,
      linksLine(page, fastext),
    );
  }
  return written.length === 0 ? undefined : written.map((line) => `${line}\n`).join("");
}
