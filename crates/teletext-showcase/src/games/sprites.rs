//! Game sprites: the sci-fi enemies taken from their gallery pictures, and
//! hand-drawn ones for the player and for when there are no pictures.
//!
//! Each picture is a whole Level 1 page of mosaic graphics showing one
//! sprite. The page's rows are read back into sixels as a receiver would
//! show them (EN 300 706 clause 12.2, the spacing attributes of Table 26,
//! and the block mosaics of Table 47), the sprite is cut out of its page,
//! and it is scaled down to a size that leaves room for a wave of them.

use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_core::consts::{
    ALPHA_WHITE, ATTRIBUTE_LAST, COLOUR_ATTRIBUTE_MASK, COLUMNS, DOUBLE_HEIGHT, HOLD_MOSAICS, MOSAIC_ATTRIBUTE_BIT, MOSAIC_BIT5, NORMAL_SIZE, RELEASE_MOSAICS,
    SPACE,
};
use neoteletext_formats::tti::TtiReader;
use neoteletext_formats::{Item, Reader};
use teletext_runtime::screen::{Colour, SIXELS_ACROSS, SIXELS_DOWN};

use super::canvas::SIXEL_BITS;
use crate::channels::Picture;

/// The picture group of the sci-fi enemy sprites (`channels::Picture`).
const SPRITE_GROUP: char = 's';
/// The picture level read: Level 1.5's rows are the Level 1 page, which
/// is all this reads (the enhancements are not needed for a sprite).
const SPRITE_LEVEL: &str = "l15";
/// The rows of a picture's page that hold the picture: below the header
/// row 0 and above the Fastext row 24 (EN 300 706 clause 9.3).
const PICTURE_ROWS: core::ops::RangeInclusive<usize> = 1..=23;
/// The picture's size in sixels.
const PICTURE_WIDTH: usize = COLUMNS * SIXELS_ACROSS;
const PICTURE_HEIGHT: usize = PICTURE_ROW_COUNT * SIXELS_DOWN;
/// Rows 1 to 23.
const PICTURE_ROW_COUNT: usize = 23;
/// Empty sixel rows that part one piece of a picture from the next. The
/// sprite sheets carry each sprite's name on one row of cells two to five
/// sixel rows under it, and a stray sixel or two; a sprite's own parts may
/// lie up to three rows apart, so the sprite is everything down to its
/// last piece taller than a row of cells (measured on the twenty
/// pictures; see `sprite_rows`).
const PIECE_GAP: usize = 2;
/// A scaled sixel is lit when at least this share of the sixels it covers
/// is: an eighth keeps the pictures' thin line art as solid creatures
/// (chosen by eye on the twenty pictures; a third lost most of them).
const LIT_NUMERATOR: usize = 1;
const LIT_DENOMINATOR: usize = 8;
/// The widest sprite, in sixels: a row is a `u16` of bits.
pub const MAX_WIDTH: usize = 16;
/// The tallest sprite, in sixels.
pub const MAX_HEIGHT: usize = 12;

/// A small one-colour mosaic sprite.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sprite {
    /// Its name, for the screens that list the enemies.
    pub name: String,
    /// Its width in sixels.
    pub width: usize,
    /// Its height in sixels.
    pub height: usize,
    /// Its colour: the colour most of the picture's lit sixels have.
    pub colour: Colour,
    /// Its rows, bit `x` the sixel `x` across.
    rows: [u16; MAX_HEIGHT],
}

impl Sprite {
    /// A sprite drawn as text, `#` a lit sixel, one string a row; rows and
    /// columns past the largest sprite are cut.
    #[must_use]
    pub fn drawn(name: &str, colour: Colour, art: &[&str]) -> Self {
        let mut rows = [0_u16; MAX_HEIGHT];
        let mut width = 0;
        for (row, line) in rows.iter_mut().zip(art) {
            for (x, character) in line.chars().take(MAX_WIDTH).enumerate() {
                if character == '#' {
                    *row |= 1_u16.checked_shl(u32::try_from(x).unwrap_or(0)).unwrap_or(0);
                }
            }
            width = width.max(line.chars().count().min(MAX_WIDTH));
        }
        Self { name: String::from(name), width, height: art.len().min(MAX_HEIGHT), colour, rows }
    }

    /// Whether the sixel at `x`, `y` is lit; outside the sprite, not.
    #[must_use]
    pub fn lit(&self, x: usize, y: usize) -> bool {
        x < self.width && self.rows.get(y).is_some_and(|row| row.checked_shr(u32::try_from(x).unwrap_or(u32::MAX)).unwrap_or(0) & 1 == 1)
    }

    /// The sprite's lit sixels.
    #[must_use]
    pub fn count(&self) -> u32 {
        self.rows.iter().map(|row| row.count_ones()).sum()
    }
}

/// The enemies: the sci-fi sprites among `pictures`, in name order, each
/// scaled to fit `width` by `height` sixels; hand-drawn ones when there
/// are none.
#[must_use]
pub fn enemies(pictures: &[Picture], width: usize, height: usize) -> Vec<Sprite> {
    let mut chosen: Vec<&Picture> = pictures.iter().filter(|picture| picture.group == SPRITE_GROUP).collect();
    chosen.sort_by(|left, right| left.name.cmp(&right.name));
    let sprites: Vec<Sprite> = chosen
        .iter()
        .filter_map(|picture| {
            let tti = picture.levels.iter().find(|(level, _)| level == SPRITE_LEVEL).or_else(|| picture.levels.first())?;
            let mut sprite = extract(&tti.1, width, height)?;
            sprite.name = title(&picture.caption);
            Some(sprite)
        })
        .collect();
    if sprites.is_empty() { drawn_enemies() } else { sprites }
}

/// A caption such as `01_razor_crawler (sci_fi_enemy_sprites_individual)`
/// as a name: `Razor crawler`.
fn title(caption: &str) -> String {
    let name = caption.split(' ').next().unwrap_or("");
    let name = name.trim_start_matches(|character: char| character.is_ascii_digit() || character == '_');
    let mut title = String::new();
    for (index, character) in name.chars().enumerate() {
        title.push(match character {
            '_' => ' ',
            _ if index == 0 => character.to_ascii_uppercase(),
            _ => character,
        });
    }
    title
}

/// An explosion.
#[must_use]
pub fn burst() -> Sprite {
    Sprite::drawn("Burst", Colour::Yellow, &["#..#..#", ".#.#.#.", "..#.#..", ".#.#.#.", "#..#..#"])
}

/// The hand-drawn enemies, for a channel without the pictures.
fn drawn_enemies() -> Vec<Sprite> {
    alloc::vec![
        Sprite::drawn("Squid", Colour::Magenta, &["...##...", "..####..", ".##..##.", "########", "..#..#..", ".#.##.#."]),
        Sprite::drawn("Crab", Colour::Cyan, &[".#....#.", "..#..#..", ".######.", "##.##.##", "########", "#.#..#.#"]),
        Sprite::drawn("Octopus", Colour::Green, &["..####..", ".######.", "##.##.##", "########", ".#.##.#.", "#......#"]),
        Sprite::drawn("Saucer", Colour::Yellow, &["..####..", ".##..##.", "########", ".#.##.#.", "........", "........"]),
        Sprite::drawn("Beetle", Colour::Red, &["#..##..#", ".######.", "##.##.##", ".######.", "#.#..#.#", "#......#"]),
    ]
}

/// The sixels of a picture, each the colour number of its foreground, 0
/// where it is not lit.
struct PictureSixels {
    colours: Vec<u8>,
}

impl PictureSixels {
    fn at(&self, x: usize, y: usize) -> u8 {
        if x >= PICTURE_WIDTH {
            return 0;
        }
        self.colours.get(y.saturating_mul(PICTURE_WIDTH).saturating_add(x)).copied().unwrap_or(0)
    }

    fn set(&mut self, x: usize, y: usize, colour: u8) {
        if x < PICTURE_WIDTH
            && let Some(slot) = self.colours.get_mut(y.saturating_mul(PICTURE_WIDTH).saturating_add(x))
        {
            *slot = colour;
        }
    }

    /// Lights the sixels of mosaic `code` in the cell at `column`, `row`
    /// (Table 47: bits 0 to 4 and 6, top-left first).
    fn mosaic(&mut self, column: usize, row: usize, code: u8, colour: u8) {
        let x = column.saturating_mul(SIXELS_ACROSS);
        let y = row.saturating_mul(SIXELS_DOWN);
        for (index, bit) in SIXEL_BITS.iter().enumerate() {
            if code & bit != 0 {
                let across = index.checked_rem(SIXELS_ACROSS).unwrap_or(0);
                let down = index.checked_div(SIXELS_ACROSS).unwrap_or(0);
                self.set(x.saturating_add(across), y.saturating_add(down), colour);
            }
        }
    }
}

/// Whether `code` shows as block mosaic in mosaic mode: bit 5 set (Table
/// 47; codes with bit 5 clear and bit 6 set are blast-through capitals).
const fn is_block_mosaic(code: u8) -> bool {
    code & MOSAIC_BIT5 != 0
}

/// Reads one row of a page as a Level 1 receiver shows it (EN 300 706
/// clause 12.2): each row starts in alphanumeric white with mosaics
/// released; colour and mode changes are set-after; Hold Mosaics is
/// set-at, and while it holds an attribute's cell shows the last mosaic
/// character instead of a space; a change between alphanumeric and mosaic
/// or of size forgets the held character.
fn read_row(codes: &[u8; COLUMNS], row: usize, picture: &mut PictureSixels) {
    let mut mosaic = false;
    let mut colour = ALPHA_WHITE;
    let mut holding = false;
    let mut held = SPACE;
    for (column, &raw) in codes.iter().enumerate() {
        let code = raw & 0x7F;
        if code > ATTRIBUTE_LAST {
            if mosaic && is_block_mosaic(code) {
                picture.mosaic(column, row, code, colour);
                held = code;
            }
            continue;
        }
        if code == HOLD_MOSAICS {
            holding = true;
        }
        if holding && mosaic {
            picture.mosaic(column, row, held, colour);
        }
        match code {
            _ if code & !COLOUR_ATTRIBUTE_MASK == 0 => {
                if mosaic {
                    held = SPACE;
                }
                mosaic = false;
                colour = code;
            }
            _ if code & !COLOUR_ATTRIBUTE_MASK == MOSAIC_ATTRIBUTE_BIT => {
                if !mosaic {
                    held = SPACE;
                }
                mosaic = true;
                colour = code & COLOUR_ATTRIBUTE_MASK;
            }
            NORMAL_SIZE | DOUBLE_HEIGHT => held = SPACE,
            RELEASE_MOSAICS => holding = false,
            _ => {}
        }
    }
}

/// The first page of `tti`, its rows as a receiver shows them in sixels.
fn read(tti: &[u8]) -> Option<PictureSixels> {
    let mut reader = TtiReader::new();
    let mut rows: Option<[[u8; COLUMNS]; PICTURE_ROW_COUNT]> = None;
    let mut take = |item: Item<'_>| {
        if let Item::Page(page) = item
            && rows.is_none()
        {
            let mut copied = [[SPACE; COLUMNS]; PICTURE_ROW_COUNT];
            for (slot, row) in copied.iter_mut().zip(PICTURE_ROWS) {
                if let Some(codes) = page.row(row) {
                    *slot = *codes;
                }
            }
            rows = Some(copied);
        }
    };
    reader.feed(tti, &mut take);
    reader.finish(&mut take);
    let rows = rows?;
    let mut picture = PictureSixels { colours: alloc::vec![0; PICTURE_WIDTH.saturating_mul(PICTURE_HEIGHT)] };
    for (row, codes) in rows.iter().enumerate() {
        read_row(codes, row, &mut picture);
    }
    Some(picture)
}

/// The rows `top..bottom` that hold the sprite: from the first lit row
/// to the end of the last piece taller than one row of cells. Pieces are
/// parted by `PIECE_GAP` empty rows; a piece no taller than a row of cells
/// below the sprite is its label or a stray, never the sprite.
fn sprite_rows(picture: &PictureSixels) -> Option<(usize, usize)> {
    let lit_row = |y: usize| (0..PICTURE_WIDTH).any(|x| picture.at(x, y) != 0);
    let mut first: Option<usize> = None;
    let mut bottom: Option<usize> = None;
    let mut piece: Option<(usize, usize)> = None;
    let mut empty = 0_usize;
    let close = |piece: (usize, usize), bottom: &mut Option<usize>| {
        if piece.1.saturating_sub(piece.0) > SIXELS_DOWN {
            *bottom = Some(piece.1);
        }
    };
    for y in 0..PICTURE_HEIGHT {
        if !lit_row(y) {
            empty = empty.saturating_add(1);
            if empty >= PIECE_GAP
                && let Some(done) = piece.take()
            {
                close(done, &mut bottom);
            }
            continue;
        }
        empty = 0;
        first = first.or(Some(y));
        piece = Some(piece.map_or((y, y.saturating_add(1)), |(top, _)| (top, y.saturating_add(1))));
    }
    if let Some(done) = piece {
        close(done, &mut bottom);
    }
    Some((first?, bottom?))
}

/// The sprite on the page of `tti`, cut out and scaled to fit `width` by
/// `height` sixels, keeping its shape; `None` when the page cannot be
/// read or shows nothing.
#[must_use]
pub fn extract(tti: &[u8], width: usize, height: usize) -> Option<Sprite> {
    let (width, height) = (width.clamp(1, MAX_WIDTH), height.clamp(1, MAX_HEIGHT));
    let picture = read(tti)?;
    let (top, bottom) = sprite_rows(&picture)?;
    let lit_columns = || (0..PICTURE_WIDTH).filter(|&x| (top..bottom).any(|y| picture.at(x, y) != 0));
    let left = lit_columns().next()?;
    let right = lit_columns().next_back()?.saturating_add(1);
    let (source_width, source_height) = (right.saturating_sub(left), bottom.saturating_sub(top));
    // Scaled down to fit the box, keeping the shape: the side that fills
    // it first sets the scale. Never up: a sixel cannot be split.
    let (width, height) = (width.min(source_width), height.min(source_height));
    let (out_width, out_height) = if source_width.saturating_mul(height) >= source_height.saturating_mul(width) {
        (width, source_height.saturating_mul(width).checked_div(source_width).unwrap_or(1).clamp(1, height))
    } else {
        (source_width.saturating_mul(height).checked_div(source_height).unwrap_or(1).clamp(1, width), height)
    };
    let mut counts = [0_usize; 8];
    let mut rows = [0_u16; MAX_HEIGHT];
    for (out_y, row) in rows.iter_mut().enumerate().take(out_height) {
        let y_from = top.saturating_add(out_y.saturating_mul(source_height).checked_div(out_height).unwrap_or(0));
        let y_to = top.saturating_add(out_y.saturating_add(1).saturating_mul(source_height).checked_div(out_height).unwrap_or(0)).max(y_from.saturating_add(1));
        for out_x in 0..out_width {
            let x_from = left.saturating_add(out_x.saturating_mul(source_width).checked_div(out_width).unwrap_or(0));
            let x_to =
                left.saturating_add(out_x.saturating_add(1).saturating_mul(source_width).checked_div(out_width).unwrap_or(0)).max(x_from.saturating_add(1));
            let mut lit = 0_usize;
            let mut area = 0_usize;
            for y in y_from..y_to {
                for x in x_from..x_to {
                    area = area.saturating_add(1);
                    let colour = picture.at(x, y);
                    if colour != 0 {
                        lit = lit.saturating_add(1);
                        if let Some(count) = counts.get_mut(usize::from(colour)) {
                            *count = count.saturating_add(1);
                        }
                    }
                }
            }
            if lit.saturating_mul(LIT_DENOMINATOR) >= area.saturating_mul(LIT_NUMERATOR) && lit > 0 {
                *row |= 1_u16.checked_shl(u32::try_from(out_x).unwrap_or(u32::MAX)).unwrap_or(0);
            }
        }
    }
    symmetrical(&mut rows, out_width);
    let colour = dominant(&counts);
    let sprite = Sprite { name: String::new(), width: out_width, height: out_height, colour, rows };
    (sprite.count() > 0).then_some(sprite)
}

/// Makes each row its own mirror image: scaled down sixfold, the
/// pictures' line art loses the symmetry their creatures have, and a wave
/// reads as a wave of creatures only with it back.
fn symmetrical(rows: &mut [u16; MAX_HEIGHT], width: usize) {
    for row in rows.iter_mut() {
        let mut mirrored = 0_u16;
        for x in 0..width {
            let bit = u32::try_from(x).unwrap_or(u32::MAX);
            if row.checked_shr(bit).unwrap_or(0) & 1 == 1 {
                let opposite = u32::try_from(width.saturating_sub(1).saturating_sub(x)).unwrap_or(u32::MAX);
                mirrored |= 1_u16.checked_shl(opposite).unwrap_or(0);
            }
        }
        *row |= mirrored;
    }
}

/// The colour with the most lit sixels. Never black, which has no
/// function as a foreground at Level 1 (the engine's decision E.18), and
/// blue only when nothing else is lit: a small blue sprite is hard to see
/// on black.
fn dominant(counts: &[usize; 8]) -> Colour {
    const COLOURS: [Colour; 8] = [Colour::Black, Colour::Red, Colour::Green, Colour::Yellow, Colour::Blue, Colour::Magenta, Colour::Cyan, Colour::White];
    let mut best = (0, Colour::Blue);
    for (count, colour) in counts.iter().zip(COLOURS).skip(1) {
        if *count > best.0 && colour != Colour::Blue {
            best = (*count, colour);
        }
    }
    best.1
}
