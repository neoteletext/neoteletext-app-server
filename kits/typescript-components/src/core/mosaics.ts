// Mosaic characters the components draw with (EN 300 706 Table 47): bit 5
// is always set; the sixels are bits 0 and 1 (top left, top right), 2 and 3
// (middle), 4 and 6 (bottom).

/** The base of every mosaic character. */
export const MOSAIC_BASE = 0x20;
/** Each sixel's bit, left then right, top to bottom. */
export const SIXEL = { topLeft: 0x01, topRight: 0x02, middleLeft: 0x04, middleRight: 0x08, bottomLeft: 0x10, bottomRight: 0x40 } as const;
/** Every sixel set. */
export const FULL_BLOCK = MOSAIC_BASE | SIXEL.topLeft | SIXEL.topRight | SIXEL.middleLeft | SIXEL.middleRight | SIXEL.bottomLeft | SIXEL.bottomRight;
/** The left column of sixels: half a cell, or a thin line down. */
export const LEFT_HALF = MOSAIC_BASE | SIXEL.topLeft | SIXEL.middleLeft | SIXEL.bottomLeft;
/** The right column of sixels. */
export const RIGHT_HALF = MOSAIC_BASE | SIXEL.topRight | SIXEL.middleRight | SIXEL.bottomRight;
/** The top row of sixels. */
export const TOP_ROW = MOSAIC_BASE | SIXEL.topLeft | SIXEL.topRight;
/** The middle row of sixels: a thin line across. */
export const MIDDLE_ROW = MOSAIC_BASE | SIXEL.middleLeft | SIXEL.middleRight;
/** The bottom row of sixels. */
export const BOTTOM_ROW = MOSAIC_BASE | SIXEL.bottomLeft | SIXEL.bottomRight;
/** The top two rows: an upper block. */
export const UPPER_TWO = TOP_ROW | MIDDLE_ROW;
/** The bottom two rows: a lower block. */
export const LOWER_TWO = MIDDLE_ROW | BOTTOM_ROW;
