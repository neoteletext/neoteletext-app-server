// Fastext links that name a sub-page, which TTI's FL line cannot say (it
// carries page numbers only, every link to any sub-page): packet X/27/0
// written whole as a packed OL,27 line, as the engine's TTI writer writes
// such links, and read back.
//
// EN 300 706 clause 9.6.1 and Figure 12: X/27/0 holds six links of six
// Hamming 8/4 bytes each (page units, page tens, S1, S2 with M1, S3, S4
// with M2 and M3), then the link control byte (Table 14) and two checksum
// bytes. In a packed line the designation code and each of those 39 bytes
// is one character whose low four bits are the byte's four data bits; the
// engine's TTI reader Hamming-codes them itself (its tti/packed.rs, the
// form vbit2 reads).

/** A link of X/27/0: a page, and the sub-code it names or null for any sub-page. */
export interface PageLink {
  readonly page: number;
  readonly subcode: number | null;
}

/** The six links of X/27/0 in order: red, green, yellow, cyan, link 5 and the index (clause 9.6.1). */
export const PACKED_LINKS = 6;
/** The packet number of the links, written as the OL line's row (clause 9.6.1). */
export const PACKED_LINKS_ROW = 27;
/** The designation code of the Fastext links among the X/27 packets (clause 9.6.1). */
const DESIGNATION = 0;
/** The bytes after the designation code: six links, the link control byte and two checksum bytes. */
const PACKED_CHARACTERS = 39;
/** The bytes of one link (Figure 12). */
const LINK_NIBBLES = 6;
/** Where the link control byte is, after the links. */
const LINK_CONTROL = PACKED_LINKS * LINK_NIBBLES;
if (LINK_CONTROL + 1 + 2 !== PACKED_CHARACTERS) {
  throw new Error("X/27/0 is six links, the link control byte and two checksum bytes");
}
/** A packed character is this plus its four data bits: printable, so the line needs no escapes (the engine's TTI_PACKED_CHARACTER_BASE). */
const PACKED_CHARACTER_BASE = 0x40;
/** The four data bits of a Hamming 8/4 byte. */
const NIBBLE_MASK = 0x0f;
const NIBBLE_BITS = 4;
/** Table 14: data bit 4 of the link control byte displays row 24; bits 1 to 3 are 1 without a local code of practice. */
const DISPLAY_ROW_24 = 0x8;
const LINK_CONTROL_RESERVED = 0x7;
/** A null link: page FF with sub-code 3F7F (clause 9.6.1). */
const NULL_PAGE = 0xff;
/** The sub-code that means any sub-page, and with page FF no link (clause 9.6.1). */
export const ANY_SUBCODE = 0x3f7f;
/** The bits of a page number within its magazine. */
const PAGE_MASK = 0xff;
const MAGAZINE_SHIFT = 8;
/** A magazine's three bits; magazine 8 is sent as 0, as in a packet's address. */
const MAGAZINE_BITS = 0x7;
const MAGAZINE_EIGHT = 8;
/** Figure 12: S1 is four bits, S2 three with M1 above, S3 four, S4 two with M2 and M3 above. */
const S2_MASK = 0x7;
const S4_MASK = 0x3;
const M1_BIT = 0x1;
const M2_M3_BITS = 0x6;
const M1_SHIFT = 3;
const M2_M3_SHIFT = 1;
/** The sub-code's bits for S2, S3 and S4 (Figure 12). */
const S2_SHIFT = 4;
const S3_SHIFT = 8;
const S4_SHIFT = 12;
/** ESC and the offset of the escape form of control codes in an OL line (the engine's TTI reader, REQ-INP-013). */
const ESCAPE = 0x1b;
const ESCAPE_OFFSET = 0x40;

/** The magazine of page number `page` as its three bits. */
function magazineBits(page: number): number {
  return (page >> MAGAZINE_SHIFT) & MAGAZINE_BITS;
}

/** One link as its six data nibbles (Figure 12), relative to the packet's magazine `ownBits`. */
function linkNibbles(link: PageLink | null, ownBits: number): number[] {
  const page = link === null ? NULL_PAGE : link.page & PAGE_MASK;
  const subcode = link === null ? ANY_SUBCODE : (link.subcode ?? ANY_SUBCODE);
  // Clause 9.6.1: the magazine bits sent are those that differ from the packet's own.
  const relative = link === null ? 0 : ownBits ^ magazineBits(link.page);
  return [
    page & NIBBLE_MASK,
    page >> NIBBLE_BITS,
    subcode & NIBBLE_MASK,
    ((subcode >> S2_SHIFT) & S2_MASK) | ((relative & M1_BIT) << M1_SHIFT),
    (subcode >> S3_SHIFT) & NIBBLE_MASK,
    ((subcode >> S4_SHIFT) & S4_MASK) | ((relative & M2_M3_BITS) << M2_M3_SHIFT),
  ];
}

/**
 * The OL,27 line, without its newline, of page `page`'s X/27/0 links:
 * six, each a link or null; the display-row-24 flag set or not. Write it
 * after the page's PN line: the reader takes the links' magazines
 * relative to the page's.
 */
export function packedLinksLine(page: number, links: readonly (PageLink | null)[], displayRow24: boolean): string {
  if (links.length !== PACKED_LINKS) {
    throw new RangeError(`X/27/0 has ${PACKED_LINKS} links`);
  }
  const ownBits = magazineBits(page);
  const nibbles = [DESIGNATION, ...links.flatMap((link) => linkNibbles(link, ownBits))];
  nibbles.push(LINK_CONTROL_RESERVED | (displayRow24 ? DISPLAY_ROW_24 : 0));
  // The two checksum bytes are not Hamming-coded, so the line cannot carry
  // them; the page's encoder computes the checksum.
  while (nibbles.length < 1 + PACKED_CHARACTERS) {
    nibbles.push(0);
  }
  return `OL,${PACKED_LINKS_ROW},${String.fromCharCode(...nibbles.map((nibble) => PACKED_CHARACTER_BASE | nibble))}`;
}

/** What an X/27/0 line says: its six links and whether row 24 is displayed. */
export interface ReadLinks {
  readonly links: readonly (PageLink | null)[];
  readonly displayRow24: boolean;
}

/**
 * Reads the text of an OL,27 line (after "OL,27,") of page `page`: its
 * links and the display-row-24 flag when it is X/27/0, else undefined. A
 * character may be in the escape form; only its low four bits count, as
 * the engine's reader takes them.
 */
export function readPackedLinks(page: number, text: string): ReadLinks | undefined {
  const nibbles: number[] = [];
  for (let at = 0; at < text.length; at += 1) {
    let code = text.charCodeAt(at);
    if (code === ESCAPE) {
      at += 1;
      code = text.charCodeAt(at) - ESCAPE_OFFSET;
    }
    nibbles.push(code & NIBBLE_MASK);
  }
  if (nibbles[0] !== DESIGNATION) {
    return undefined;
  }
  const value = (index: number): number => nibbles[1 + index] ?? 0;
  const ownBits = magazineBits(page);
  const links = Array.from({ length: PACKED_LINKS }, (_, link): PageLink | null => {
    const start = link * LINK_NIBBLES;
    const pageByte = value(start) | (value(start + 1) << NIBBLE_BITS);
    const s2 = value(start + 3);
    const s4 = value(start + 5);
    const subcode = value(start + 2) | ((s2 & S2_MASK) << S2_SHIFT) | (value(start + 4) << S3_SHIFT) | ((s4 & S4_MASK) << S4_SHIFT);
    if (pageByte === NULL_PAGE && subcode === ANY_SUBCODE) {
      return null;
    }
    const relative = (s2 >> M1_SHIFT) | ((s4 >> M2_M3_SHIFT) & M2_M3_BITS);
    const bits = ownBits ^ relative;
    const magazine = bits === 0 ? MAGAZINE_EIGHT : bits;
    return { page: (magazine << MAGAZINE_SHIFT) | pageByte, subcode: subcode === ANY_SUBCODE ? null : subcode };
  });
  return { links, displayRow24: (value(LINK_CONTROL) & DISPLAY_ROW_24) !== 0 };
}
