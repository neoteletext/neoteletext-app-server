// Text as the teletext English G0 set can show it (EN 300 706 Table 36,
// the English national option subset): each character becomes the code
// that shows it, or the nearest the set has.
//
// The English set shows other signs at ten codes where ASCII has
// punctuation (0x23, 0x5B to 0x60, 0x7B to 0x7E), so ASCII's `[`, `\`,
// `]`, `^`, `_`, `` ` ``, `{`, `|`, `}` and `~` are folded to the nearest
// sign it does have rather than written at their ASCII codes, where a
// receiver would show an arrow, a fraction or `#` instead.

/** The codes of EN 300 706 Table 36's English column that differ from ASCII. */
export const ENGLISH = {
  pound: 0x23,
  leftArrow: 0x5b,
  half: 0x5c,
  rightArrow: 0x5d,
  upArrow: 0x5e,
  hash: 0x5f,
  dash: 0x60,
  quarter: 0x7b,
  doubleBar: 0x7c,
  threeQuarters: 0x7d,
  divide: 0x7e,
} as const;

/** The code of a space (EN 300 706 Table 35). */
export const SPACE_CODE = 0x20;
/** The first printable code of the G0 set. */
const FIRST_PRINTABLE = 0x20;
/** The last code below DEL, which the G0 set shows as a block. */
const LAST_PRINTABLE = 0x7e;
/** DEL, then the C1 controls to U+009F: control characters, as those below 0x20 are. */
const DELETE = 0x7f;
const LAST_C1_CONTROL = 0x9f;
/** What a character the set cannot show, nor fold to one it can, becomes. */
export const UNSHOWN = "?";

/** The signs Table 36's English column has, by the character they show. */
const SIGNS: ReadonlyMap<string, number> = new Map([
  ["£", ENGLISH.pound],
  ["#", ENGLISH.hash],
  ["←", ENGLISH.leftArrow],
  ["½", ENGLISH.half],
  ["→", ENGLISH.rightArrow],
  ["↑", ENGLISH.upArrow],
  ["—", ENGLISH.dash],
  ["―", ENGLISH.dash],
  ["¼", ENGLISH.quarter],
  ["‖", ENGLISH.doubleBar],
  ["¾", ENGLISH.threeQuarters],
  ["÷", ENGLISH.divide],
]);

/**
 * Characters folded to what the set has: the ASCII signs whose codes the
 * English set uses for others, quotes and dashes, and the Latin letters
 * with diacritics, without them.
 */
const FOLDS: ReadonlyMap<string, string> = new Map([
  ["[", "("],
  ["{", "("],
  ["]", ")"],
  ["}", ")"],
  ["\\", "/"],
  ["|", "/"],
  ["^", " "],
  ["_", " "],
  ["`", "'"],
  ["~", "-"],
  ["·", "-"],
  ["‐", "-"],
  ["‑", "-"],
  ["‒", "-"],
  ["–", "-"],
  ["−", "-"],
  ["‘", "'"],
  ["’", "'"],
  ["‚", "'"],
  ["′", "'"],
  ["“", '"'],
  ["”", '"'],
  ["„", '"'],
  ["″", '"'],
  ["…", "..."],
  ["×", "x"],
  ["°", " deg"],
  ["©", "(c)"],
  ["Æ", "AE"],
  ["æ", "ae"],
  ["Œ", "OE"],
  ["œ", "oe"],
  ["ß", "ss"],
  ["Þ", "Th"],
  ["þ", "th"],
  ...letters("A", "ÀÁÂÃÄÅĀĂĄ"),
  ...letters("a", "àáâãäåāăąả"),
  ...letters("C", "ÇĆĈĊČ"),
  ...letters("c", "çćĉċč"),
  ...letters("D", "ĎĐÐ"),
  ...letters("d", "ďđð"),
  ...letters("E", "ÈÉÊËĒĖĘĚ"),
  ...letters("e", "èéêëēėęě"),
  ...letters("G", "ĞĢ"),
  ...letters("g", "ğģ"),
  ...letters("I", "ÌÍÎÏĪİ"),
  ...letters("i", "ìíîïīı"),
  ...letters("L", "ŁĽĹ"),
  ...letters("l", "łľĺ"),
  ...letters("N", "ÑŃŇ"),
  ...letters("n", "ñńň"),
  ...letters("O", "ÒÓÔÕÖØŌŐ"),
  ...letters("o", "òóôõöøōő"),
  ...letters("R", "Ř"),
  ...letters("r", "ř"),
  ...letters("S", "ŚŠŞ"),
  ...letters("s", "śšş"),
  ...letters("T", "ŤŢ"),
  ...letters("t", "ťţ"),
  ...letters("U", "ÙÚÛÜŪŮŰ"),
  ...letters("u", "ùúûüūůűứ"),
  ...letters("Y", "ÝŸ"),
  ...letters("y", "ýÿ"),
  ...letters("Z", "ŹŻŽ"),
  ...letters("z", "źżž"),
]);

/** Each of `accented` folded to `base`. */
function letters(base: string, accented: string): [string, string][] {
  return [...accented].map((letter) => [letter, base]);
}

/** The Russian alphabet from U+0430, in the BGN/PCGN romanisation. */
const CYRILLIC = [
  "a", "b", "v", "g", "d", "e", "zh", "z", "i", "y", "k", "l", "m", "n", "o", "p",
  "r", "s", "t", "u", "f", "kh", "ts", "ch", "sh", "shch", "", "y", "", "e", "yu", "ya",
] as const;
/** The Cyrillic letters of other languages, from U+0450. */
const CYRILLIC_EXTRA = ["e", "yo", "dj", "g", "ye", "dz", "i", "yi", "j", "lj", "nj", "c", "k", "i", "u", "dz"] as const;
/** Where each table's small letters start, and its capitals (Unicode's Cyrillic block). */
const CYRILLIC_LOWER = 0x0430;
const CYRILLIC_UPPER = 0x0410;
const CYRILLIC_EXTRA_LOWER = 0x0450;
const CYRILLIC_EXTRA_UPPER = 0x0400;
/** Combining diacritical marks, U+0300 to U+036F: dropped, the letter shown without them. */
const COMBINING_FIRST = 0x0300;
const COMBINING_LAST = 0x036f;

/** A Cyrillic letter romanised, capitalised as the letter was; undefined for any other character. */
function romanised(point: number): string | undefined {
  const tables: [readonly string[], number, boolean][] = [
    [CYRILLIC, CYRILLIC_LOWER, false],
    [CYRILLIC, CYRILLIC_UPPER, true],
    [CYRILLIC_EXTRA, CYRILLIC_EXTRA_LOWER, false],
    [CYRILLIC_EXTRA, CYRILLIC_EXTRA_UPPER, true],
  ];
  for (const [table, first, capital] of tables) {
    const letter = point >= first ? table[point - first] : undefined;
    if (letter !== undefined) {
      return capital ? letter.charAt(0).toUpperCase() + letter.slice(1) : letter;
    }
  }
  return undefined;
}

/**
 * The characters `character` (one code point) is shown as: itself where
 * the set has it, its fold, "" for a combining mark, or undefined where
 * the set has nothing near it.
 */
export function foldCharacter(character: string): string | undefined {
  if (SIGNS.has(character)) {
    return character;
  }
  const fold = FOLDS.get(character);
  if (fold !== undefined) {
    return fold;
  }
  const point = character.codePointAt(0) ?? 0;
  if (point >= FIRST_PRINTABLE && point <= LAST_PRINTABLE) {
    return character;
  }
  if (point >= COMBINING_FIRST && point <= COMBINING_LAST) {
    return "";
  }
  if (/^\s$/u.test(character)) {
    return " ";
  }
  return romanised(point);
}

/**
 * `text` as codes of the English G0 set, one or more a character: what
 * the set cannot show becomes `?`, a control character a space.
 */
export function englishCodes(text: string): number[] {
  const codes: number[] = [];
  for (const character of text) {
    const shown = foldCharacter(character);
    if (shown === undefined) {
      const point = character.codePointAt(0) ?? 0;
      const control = point < FIRST_PRINTABLE || (point >= DELETE && point <= LAST_C1_CONTROL);
      codes.push(control ? SPACE_CODE : UNSHOWN.charCodeAt(0));
      continue;
    }
    for (const piece of shown) {
      codes.push(SIGNS.get(piece) ?? piece.charCodeAt(0));
    }
  }
  return codes;
}
