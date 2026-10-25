// Space invaders, with the sci-fi sprites as the enemies: each row of a
// wave is a different creature, and each wave brings the next ones
// (crates/teletext-showcase/src/games/invaders.rs).
//
// Everything is in sixels on the 78 by 63 playfield. Rows of different
// colours keep a row of cells apart, and enemies, shields and the player
// each sit on whole rows of cells, so every colour shows (see canvas.ts).

import { Colour, SIXELS_DOWN, type Screen } from "../kit.ts";
import { Effect } from "../sound.ts";
import type { Canvas } from "./canvas.ts";
import * as look from "./look.ts";
import { FIELD_HEIGHT, FIELD_WIDTH } from "./look.ts";
import { Control, type Dice, Game, type Keys, between } from "./pages.ts";
import { Sprite, burst } from "./sprites.ts";

/** A wave's rows and columns of enemies. */
export const ENEMY_ROWS = 4;
const ENEMY_COLUMNS = 5;
/** An enemy's largest size in sixels: five columns of ten with three between leave the wave sixteen sixels to march across. */
export const ENEMY_WIDTH = 10;
export const ENEMY_HEIGHT = 6;
/** Enemies' spacing: across, the width and a gap; down, three rows of cells, so each row of enemies keeps to its own two rows of cells and its colour. */
const PITCH_X = 13;
const PITCH_Y = 9;
/** What an enemy of each row is worth, top row first. */
export const POINTS: readonly number[] = [40, 30, 20, 10];
/** The wave's first row, one row of cells under the saucer's; each later wave starts a row of cells lower, three times at most. */
const WAVE_TOP = 6;
const LOWER_WAVES = 3;
/** How far the wave drops at an edge: a row of cells, so it stays on whole rows. */
const DROP = SIXELS_DOWN;
/** The player's ship, on the last row of cells. */
const PLAYER_Y = FIELD_HEIGHT - SIXELS_DOWN;
const PLAYER_SPEED = 2;
/** The shields: three, spread evenly, on two rows of cells above the player. */
const SHIELD_Y = 48;
const SHIELD_LEFT: readonly number[] = [7, 33, 59];
const SHIELD_WIDTH = 12;
const SHIELD_SHAPE: readonly number[] = [0b0001_1111_1000, 0b0111_1111_1110, 0b1111_1111_1111, 0b1111_1111_1111, 0b1111_0000_1111, 0b1110_0000_0111];
/** The player's shot climbs this many sixels a tick. */
const SHOT_SPEED = 3;
/** Bombs: at most this many falling at once. */
const MAX_BOMBS = 4;
/** From this wave (counted from 0) bombs fall two sixels a tick, not one. */
const FAST_BOMB_WAVE = 2;
const BOMB_SPEED = 1;
const FAST_BOMB_SPEED = 2;
/** The chance of a new bomb each tick, in thirtieths: three, and one more each wave, up to thirteen. */
const BOMB_CHANCE_OUT_OF = 30;
const BOMB_CHANCE_BASE = 3;
const BOMB_CHANCE_MOST = 13;
/** The banners' row: under the wave, above the shields. */
export const BANNER_ROW = 15;
/** Lives at the start. */
const LIVES = 3;
/** Ticks the player's explosion lasts before the next ship. */
const DYING_TICKS = 15;
/** Ticks a wave's banner shows before the wave moves. */
const BANNER_TICKS = 15;
/** Ticks a burst shows. */
const BURST_TICKS = 3;
const MAX_BURSTS = 4;
/** The saucer: on the top row of cells, every 15 to 30 seconds, worth 50 to 300. */
const SAUCER_Y = 0;
const SAUCER_EVERY: readonly [number, number] = [150, 300];
const SAUCER_POINTS: readonly [number, number] = [1, 6];
const SAUCER_WIDTH = 8;
const SAUCER_HEIGHT = 3;
const SAUCER_POINT_STEP = 50;
/** The wave marches one step every few ticks: one tick each seven enemies left, less one for each wave after the first two, never under one. */
const ENEMIES_PER_TICK = 7;
/** The player's ship's width, and where its gun is. */
const SHIP_WIDTH = 7;
const GUN = 3;
/** The rows above the player's ship where its explosion shows. */
const BURST_ABOVE_SHIP = 2;
/** A shot's and a bomb's length, in sixels. */
const MISSILE_LENGTH = 2;
/** The bits of a shield row, and of a wave's enemies alive. */
const WORD_BITS = 32;

function playerShip(): Sprite {
  return Sprite.drawn("Ship", Colour.Green, ["...#...", ".#####.", "#######"]);
}

function saucerSprite(): Sprite {
  return Sprite.drawn("Saucer", Colour.Magenta, ["..####..", ".#.##.#.", "########"]);
}

/** The enemy of `row` in `wave`: each wave takes the next four creatures. */
export function enemy(enemies: readonly Sprite[], wave: number, row: number): Sprite | undefined {
  return enemies.length === 0 ? undefined : enemies[(wave * ENEMY_ROWS + row) % enemies.length];
}

interface Point {
  readonly x: number;
  readonly y: number;
}

/** The saucer crossing the top: where, which way, and its worth. */
interface Saucer {
  readonly at: Point;
  readonly heading: number;
  readonly worth: number;
}

/** Whether bit `index` of `bits` is set; outside, not. */
function bitSet(bits: number, index: number): boolean {
  return index >= 0 && index < WORD_BITS && ((bits >>> index) & 1) === 1;
}

/** `bits` with bit `index` cleared. */
function bitCleared(bits: number, index: number): number {
  return index >= 0 && index < WORD_BITS ? bits & ~(1 << index) : bits;
}

/** The number of bits set in `bits`. */
function countOnes(bits: number): number {
  let count = 0;
  for (let rest = bits; rest !== 0; rest &= rest - 1) {
    count += 1;
  }
  return count;
}

/** The left edge that centres something `width` wide on the playfield. */
function centred(width: number): number {
  return Math.trunc((FIELD_WIDTH - width) / 2);
}

/** The shields as they stand at a wave's start. */
function mendedShields(): number[][] {
  return SHIELD_LEFT.map(() => [...SHIELD_SHAPE]);
}

/** A game of invaders. */
export class Invaders {
  #score = 0;
  #lives = LIVES;
  /** The wave, from 0. */
  #wave = 0;
  /** The enemies alive, bit `row * ENEMY_COLUMNS + column`. */
  #alive = 0;
  /** The wave's top-left, and which way it marches: 1 right, -1 left. */
  #formation: Point = { x: 0, y: 0 };
  #heading = 1;
  #marchWait = 0;
  /** The walking frame, flipped at each step. */
  #stride = false;
  #playerX = 0;
  #shot: Point | null = null;
  #bombs: (Point | null)[] = new Array<Point | null>(MAX_BOMBS).fill(null);
  #shields: number[][] = mendedShields();
  #bursts: ({ readonly at: Point; readonly ticks: number } | null)[] = new Array<null>(MAX_BURSTS).fill(null);
  #saucer: Saucer | null = null;
  #saucerWait = SAUCER_EVERY[1];
  #dying = 0;
  #banner = BANNER_TICKS;
  #over = false;

  /** A new game at its first wave. */
  constructor() {
    this.#startWave();
  }

  /** Lines up the wave: all alive, at the left, the shields mended. */
  #startWave(): void {
    this.#alive = 2 ** (ENEMY_ROWS * ENEMY_COLUMNS) - 1;
    const lower = Math.min(this.#wave, LOWER_WAVES) * DROP;
    this.#formation = { x: 0, y: WAVE_TOP + lower };
    this.#heading = 1;
    this.#shields = mendedShields();
    this.#playerX = centred(SHIP_WIDTH);
    this.#shot = null;
    this.#bombs = new Array<Point | null>(MAX_BOMBS).fill(null);
    this.#banner = BANNER_TICKS;
  }

  /** The score. */
  score(): number {
    return this.#score;
  }

  /** Whether the game is over. */
  over(): boolean {
    return this.#over;
  }

  /** Lives left. */
  lives(): number {
    return this.#lives;
  }

  /** Every enemy of the wave shot down at once, for tests. */
  clearWave(): void {
    this.#alive = 0;
  }

  /** The wave, from 0. */
  wave(): number {
    return this.#wave;
  }

  /** The player's ship's left edge. */
  playerX(): number {
    return this.#playerX;
  }

  static #bit(row: number, column: number): number {
    return 1 << (row * ENEMY_COLUMNS + column);
  }

  #isAlive(row: number, column: number): boolean {
    return (this.#alive & Invaders.#bit(row, column)) !== 0;
  }

  /** The top-left of the enemy at `row`, `column`. */
  #enemyAt(row: number, column: number): Point {
    return { x: this.#formation.x + column * PITCH_X, y: this.#formation.y + row * PITCH_Y };
  }

  /** The enemy whose box holds `point`. */
  #enemyHit(point: Point): readonly [number, number] | undefined {
    for (let row = 0; row < ENEMY_ROWS; row += 1) {
      for (let column = 0; column < ENEMY_COLUMNS; column += 1) {
        const at = this.#enemyAt(row, column);
        if (this.#isAlive(row, column) && point.x >= at.x && point.x < at.x + ENEMY_WIDTH && point.y >= at.y && point.y < at.y + ENEMY_HEIGHT) {
          return [row, column];
        }
      }
    }
    return undefined;
  }

  #burst(at: Point): void {
    const slot = this.#bursts.indexOf(null);
    if (slot !== -1) {
      this.#bursts[slot] = { at, ticks: BURST_TICKS };
    }
  }

  /** Whether the shield sixel at `point` stands; if so it is knocked out with its four neighbours. */
  #shieldHit(point: Point): boolean {
    const row = point.y - SHIELD_Y;
    if (row < 0) {
      return false;
    }
    for (const [index, shield] of this.#shields.entries()) {
      const column = point.x - (SHIELD_LEFT[index] ?? 0);
      const bits = shield[row];
      if (bits === undefined || column < 0 || column >= SHIELD_WIDTH || !bitSet(bits, column)) {
        continue;
      }
      for (const [dx, dy] of [
        [0, 0],
        [-1, 0],
        [1, 0],
        [0, -1],
        [0, 1],
      ] as const) {
        const [x, y] = [column + dx, row + dy];
        const neighbour = shield[y];
        if (y >= 0 && x >= 0 && x < SHIELD_WIDTH && neighbour !== undefined) {
          shield[y] = bitCleared(neighbour, x);
        }
      }
      return true;
    }
    return false;
  }

  /** One tick: whether the screen changed. */
  tick(keys: Keys, dice: Dice, sounds: Effect[]): boolean {
    if (this.#over) {
      return false;
    }
    this.#ageBursts();
    // While the wave's banner shows, only the ship moves.
    if (this.#banner > 0) {
      this.#banner -= 1;
      this.#movePlayer(keys, false, sounds);
      return true;
    }
    this.#movePlayer(keys, true, sounds);
    this.#moveShot(sounds);
    this.#moveBombs(sounds);
    this.#march(sounds);
    this.#dropBombs(dice);
    this.#flySaucer(dice);
    if (this.#alive === 0 && !this.#over) {
      this.#wave += 1;
      this.#startWave();
      sounds.push(Effect.Win);
    }
    return true;
  }

  #ageBursts(): void {
    this.#bursts = this.#bursts.map((slot) => (slot !== null && slot.ticks > 1 ? { at: slot.at, ticks: slot.ticks - 1 } : null));
  }

  #movePlayer(keys: Keys, mayFire: boolean, sounds: Effect[]): void {
    if (this.#dying > 0) {
      this.#dying -= 1;
      if (this.#dying === 0) {
        this.#playerX = centred(SHIP_WIDTH);
        this.#bombs = new Array<Point | null>(MAX_BOMBS).fill(null);
      }
      return;
    }
    const widest = FIELD_WIDTH - SHIP_WIDTH;
    if (keys.active(Control.Left)) {
      this.#playerX = Math.max(this.#playerX - PLAYER_SPEED, 0);
    }
    if (keys.active(Control.Right)) {
      this.#playerX = Math.min(this.#playerX + PLAYER_SPEED, widest);
    }
    if (mayFire && keys.active(Control.Fire) && this.#shot === null) {
      this.#shot = { x: this.#playerX + GUN, y: PLAYER_Y - 1 };
      sounds.push(Effect.Laser);
    }
  }

  #moveShot(sounds: Effect[]): void {
    for (let step = 0; step < SHOT_SPEED; step += 1) {
      const shot = this.#shot;
      if (shot === null) {
        return;
      }
      const next = { x: shot.x, y: shot.y - 1 };
      this.#shot = next;
      if (next.y < 0) {
        this.#shot = null;
        continue;
      }
      const saucer = this.#saucer;
      if (saucer !== null && next.x >= saucer.at.x && next.x < saucer.at.x + SAUCER_WIDTH && next.y < saucer.at.y + SAUCER_HEIGHT) {
        this.#saucer = null;
        this.#shot = null;
        this.#score += saucer.worth;
        this.#burst(saucer.at);
        sounds.push(Effect.Coin);
        continue;
      }
      const hit = this.#enemyHit(next);
      if (hit !== undefined) {
        const [row, column] = hit;
        this.#alive &= ~Invaders.#bit(row, column);
        this.#score += POINTS[row] ?? 0;
        this.#shot = null;
        this.#burst(this.#enemyAt(row, column));
        sounds.push(Effect.Explosion);
        continue;
      }
      if (this.#shieldHit(next)) {
        this.#shot = null;
        continue;
      }
      // A shot meets a bomb falling in its column, at its sixel or the
      // one under, and both are gone.
      const bomb = this.#bombs.findIndex((each) => each !== null && each.x === next.x && (each.y === next.y || each.y === next.y + 1));
      if (bomb !== -1) {
        this.#bombs[bomb] = null;
        this.#shot = null;
      }
    }
  }

  #moveBombs(sounds: Effect[]): void {
    const speed = this.#wave >= FAST_BOMB_WAVE ? FAST_BOMB_SPEED : BOMB_SPEED;
    for (let index = 0; index < MAX_BOMBS; index += 1) {
      for (let step = 0; step < speed; step += 1) {
        const bomb = this.#bombs[index];
        if (bomb === undefined || bomb === null) {
          break;
        }
        const next = { x: bomb.x, y: bomb.y + 1 };
        let keep: Point | null = next;
        if (next.y >= FIELD_HEIGHT || this.#shieldHit(next)) {
          keep = null;
        } else if (this.#dying === 0 && next.y >= PLAYER_Y && next.x >= this.#playerX && next.x < this.#playerX + SHIP_WIDTH) {
          keep = null;
          this.#loseLife(sounds);
        }
        this.#bombs[index] = keep;
      }
    }
  }

  #loseLife(sounds: Effect[]): void {
    this.#lives = Math.max(0, this.#lives - 1);
    this.#dying = DYING_TICKS;
    this.#burst({ x: this.#playerX, y: PLAYER_Y - BURST_ABOVE_SHIP });
    sounds.push(Effect.Explosion);
    if (this.#lives === 0) {
      this.#over = true;
      sounds.push(Effect.Lose);
    }
  }

  /**
   * The wave's step: across, or down at an edge; an enemy that reaches
   * the shields wrecks them, and one that reaches the player's row ends
   * the game.
   */
  #march(sounds: Effect[]): void {
    if (this.#marchWait > 0) {
      this.#marchWait -= 1;
      return;
    }
    // From the third wave on, each wave marches a tick faster.
    const faster = Math.max(0, this.#wave - 1);
    this.#marchWait = Math.max(0, Math.floor(countOnes(this.#alive) / ENEMIES_PER_TICK) - faster);
    const columns: number[] = [];
    for (let column = 0; column < ENEMY_COLUMNS; column += 1) {
      if (this.#columnAlive(column)) {
        columns.push(column);
      }
    }
    const [first, last] = [columns[0], columns[columns.length - 1]];
    if (first === undefined || last === undefined) {
      return;
    }
    const left = this.#enemyAt(0, first).x;
    const right = this.#enemyAt(0, last).x + ENEMY_WIDTH;
    const atEdge = (this.#heading > 0 && right >= FIELD_WIDTH) || (this.#heading < 0 && left <= 0);
    if (atEdge) {
      this.#formation = { x: this.#formation.x, y: this.#formation.y + DROP };
      this.#heading = -this.#heading;
    } else {
      this.#formation = { x: this.#formation.x + this.#heading, y: this.#formation.y };
    }
    this.#stride = !this.#stride;
    const lowest = this.#lowestRow();
    if (lowest !== undefined) {
      const bottom = this.#enemyAt(lowest, 0).y + ENEMY_HEIGHT;
      if (bottom > SHIELD_Y) {
        const wrecked = bottom - SHIELD_Y;
        for (const shield of this.#shields) {
          shield.fill(0, 0, Math.min(wrecked, shield.length));
        }
      }
      if (bottom > PLAYER_Y && !this.#over) {
        this.#over = true;
        sounds.push(Effect.Lose);
      }
    }
  }

  #columnAlive(column: number): boolean {
    for (let row = 0; row < ENEMY_ROWS; row += 1) {
      if (this.#isAlive(row, column)) {
        return true;
      }
    }
    return false;
  }

  /** The lowest row with an enemy alive. */
  #lowestRow(): number | undefined {
    for (let row = ENEMY_ROWS - 1; row >= 0; row -= 1) {
      for (let column = 0; column < ENEMY_COLUMNS; column += 1) {
        if (this.#isAlive(row, column)) {
          return row;
        }
      }
    }
    return undefined;
  }

  #dropBombs(dice: Dice): void {
    const chance = Math.min(BOMB_CHANCE_BASE + this.#wave, BOMB_CHANCE_MOST);
    if (dice.below(BOMB_CHANCE_OUT_OF) >= chance) {
      return;
    }
    const slot = this.#bombs.indexOf(null);
    if (slot === -1) {
      return;
    }
    const column = dice.below(ENEMY_COLUMNS);
    let row: number | undefined;
    for (let each = ENEMY_ROWS - 1; each >= 0 && row === undefined; each -= 1) {
      if (this.#isAlive(each, column)) {
        row = each;
      }
    }
    if (row === undefined) {
      return;
    }
    const at = this.#enemyAt(row, column);
    const half = Math.floor(ENEMY_WIDTH / 2);
    this.#bombs[slot] = { x: at.x + half, y: at.y + ENEMY_HEIGHT };
  }

  #flySaucer(dice: Dice): void {
    const saucer = this.#saucer;
    if (saucer !== null) {
      const next = saucer.at.x + saucer.heading;
      this.#saucer = next > -SAUCER_WIDTH && next < FIELD_WIDTH ? { ...saucer, at: { x: next, y: saucer.at.y } } : null;
      return;
    }
    this.#saucerWait -= 1;
    if (this.#saucerWait <= 0) {
      this.#saucerWait = between(dice, SAUCER_EVERY[0], SAUCER_EVERY[1]);
      const worth = between(dice, SAUCER_POINTS[0], SAUCER_POINTS[1]) * SAUCER_POINT_STEP;
      // In from the left, its first column showing, or from the right.
      const [x, heading] = dice.below(2) === 0 ? [-(SAUCER_WIDTH - 1), 1] : [FIELD_WIDTH - 1, -1];
      this.#saucer = { at: { x, y: SAUCER_Y }, heading, worth };
    }
  }

  /** Draws the game: status bar, playfield and controls. */
  draw(screen: Screen, enemies: readonly Sprite[]): void {
    look.status(screen, Game.Invaders, [
      [Colour.Yellow, `SCORE ${look.digits(this.#score)}`],
      [Colour.Cyan, `WAVE ${this.#wave + 1}`],
      [Colour.White, `LIVES ${this.#lives}`],
    ]);
    const field = look.field();
    if (this.#saucer !== null) {
      field.sprite(saucerSprite(), this.#saucer.at.x, this.#saucer.at.y, Colour.Magenta);
    }
    for (let row = 0; row < ENEMY_ROWS; row += 1) {
      const sprite = enemy(enemies, this.#wave, row);
      if (sprite === undefined) {
        continue;
      }
      for (let column = 0; column < ENEMY_COLUMNS; column += 1) {
        if (this.#isAlive(row, column)) {
          const at = this.#enemyAt(row, column);
          const inset = Math.floor(Math.max(0, ENEMY_WIDTH - sprite.width) / 2);
          drawWalking(field, sprite, { x: at.x + inset, y: at.y }, this.#stride);
        }
      }
    }
    this.#shields.forEach((shield, index) => {
      const left = SHIELD_LEFT[index] ?? 0;
      shield.forEach((bits, row) => {
        for (let column = 0; column < SHIELD_WIDTH; column += 1) {
          if (bitSet(bits, column)) {
            field.plot(left + column, SHIELD_Y + row, Colour.Green);
          }
        }
      });
    });
    if (this.#dying === 0) {
      field.sprite(playerShip(), this.#playerX, PLAYER_Y, Colour.Green);
    }
    if (this.#shot !== null) {
      field.rectangle(this.#shot.x, this.#shot.y, 1, MISSILE_LENGTH, Colour.White);
    }
    for (const bomb of this.#bombs) {
      if (bomb !== null) {
        field.rectangle(bomb.x, bomb.y - 1, 1, MISSILE_LENGTH, Colour.Yellow);
      }
    }
    for (const slot of this.#bursts) {
      if (slot !== null) {
        field.sprite(burst(), slot.at.x, slot.at.y, Colour.Yellow);
      }
    }
    field.draw(screen, look.FIELD_ROW, look.FIELD_COLUMN);
    if (this.#banner > 0 && !this.#over) {
      const name = enemy(enemies, this.#wave, 0)?.name ?? "the invaders";
      look.banner(screen, BANNER_ROW, Colour.Blue, `WAVE ${this.#wave + 1}`, `Here come the ${name}s`);
    }
    look.hint(screen, "← → move   Space fires   Esc menu");
  }
}

/** Draws an enemy in its colour; on the second stride its lowest two rows step outwards a sixel, so the wave walks. */
function drawWalking(field: Canvas, sprite: Sprite, at: Point, stride: boolean): void {
  const middle = Math.floor(sprite.width / 2);
  const feet = Math.max(0, sprite.height - 2);
  for (let y = 0; y < sprite.height; y += 1) {
    for (let x = 0; x < sprite.width; x += 1) {
      if (!sprite.lit(x, y)) {
        continue;
      }
      let column = x;
      if (stride && y >= feet) {
        column = x < middle ? column - 1 : column + 1;
      }
      field.plot(at.x + column, at.y + y, sprite.colour);
    }
  }
}
