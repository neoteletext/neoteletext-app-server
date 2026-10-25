// Sounds as a program sends them (docs/applications.md section 3.4): the
// engine's audio profile, with samples as base64 of 16-bit little-endian
// mono PCM, which the server encodes as IMA ADPCM. Each member is checked
// here as the server checks it, so a mistake is caught where it is made
// rather than refused in the server's log.

import { isPageNumber, pageText } from "./screen.ts";

/** The profile's sample rates (docs/applications.md section 3.4; the engine's `audio.md`). */
export const SAMPLE_RATES = [8_000, 11_025, 16_000, 22_050, 24_000, 32_000, 44_100, 48_000] as const;
/** A sample rate of the profile. */
export type SampleRate = (typeof SAMPLE_RATES)[number];
/** The profile's channels: 0 to 3 (audio.md section 4.4). */
export const AUDIO_CHANNELS = 4;
/** The highest handle: one byte (audio.md section 4.2). */
export const LAST_HANDLE = 255;
/** The highest generation: two bytes. */
export const LAST_GENERATION = 0xffff;
/** Full gain (audio.md section 4.4: 0 to 100). */
export const FULL_GAIN = 100;
/** The longest fade: two bytes of milliseconds. */
export const LONGEST_FADE_MS = 0xffff;
/** The deepest duck step (audio.md section 4.4: 0 to 6). */
export const DEEPEST_DUCK = 6;
/** The longest sound one message may carry: two minutes at 48 kHz, the browser's limit (docs/applications.md section 3.4). */
export const MOST_SAMPLES = 48_000 * 120;
/** The highest block number of a live sound: two bytes. */
const LAST_BLOCK = 0xffff;
/** Bytes in a 16-bit sample. */
const SAMPLE_BYTES = 2;

/** Who hears a sound: one viewer, or everyone with a page open. */
export type Audience = { readonly session: number } | { readonly page: number };

/** A stored sound: its samples, sent once and played by later commands. */
export interface SoundObject {
  readonly handle: number;
  readonly generation: number;
  readonly rate: SampleRate;
  readonly samples: Int16Array;
}

/** A live sound's object, whose samples follow as blocks (audio.md section 8.3). */
export interface LiveSound {
  readonly handle: number;
  readonly generation: number;
  readonly rate: SampleRate;
}

/** Live blocks from block number `first`, of 1,017 samples each. */
export interface LiveBlocks extends LiveSound {
  readonly first: number;
  readonly samples: Int16Array;
}

/** A command to play a stored or live sound on a channel. */
export interface Play {
  readonly channel: number;
  readonly handle: number;
  readonly generation: number;
  /** 0 to 100; 100 when absent. */
  readonly gain?: number;
  readonly fadeMs?: number;
  readonly loop?: boolean;
  /** Start after the sound playing on the channel ends. */
  readonly after?: boolean;
  /** How far other channels duck, 0 to 6. */
  readonly duck?: number;
  /** The page the sound belongs to, or none for the whole service. */
  readonly page?: number;
}

/** A command to stop a channel. */
export interface Stop {
  readonly channel: number;
  readonly fadeMs?: number;
}

/** A command to change a channel's gain. */
export interface Gain {
  readonly channel: number;
  readonly gain: number;
  readonly fadeMs?: number;
}

/** One `sound` message's member. */
export type SoundMember =
  | { readonly object: SoundObject }
  | { readonly live: LiveSound }
  | { readonly blocks: LiveBlocks }
  | { readonly play: Play }
  | { readonly stop: Stop }
  | { readonly gain: Gain };

/** `samples` as base64 of 16-bit little-endian samples (RFC 4648 section 4). */
export function pcmBase64(samples: Int16Array): string {
  const bytes = new DataView(new ArrayBuffer(samples.length * SAMPLE_BYTES));
  samples.forEach((sample, index) => bytes.setInt16(index * SAMPLE_BYTES, sample, true));
  return Buffer.from(bytes.buffer).toString("base64");
}

function wholeIn(name: string, value: number, least: number, most: number): number {
  if (!Number.isInteger(value) || value < least || value > most) {
    throw new RangeError(`sound: ${name} is ${least} to ${most}, not ${value}`);
  }
  return value;
}

function handleFields(sound: { readonly handle: number; readonly generation: number }): Record<string, number> {
  return { handle: wholeIn("handle", sound.handle, 0, LAST_HANDLE), generation: wholeIn("generation", sound.generation, 0, LAST_GENERATION) };
}

function rateOf(rate: number): number {
  if (!(SAMPLE_RATES as readonly number[]).includes(rate)) {
    throw new RangeError(`sound: ${rate} is not one of the profile's rates`);
  }
  return rate;
}

function samplesOf(samples: Int16Array): string {
  if (samples.length > MOST_SAMPLES) {
    throw new RangeError(`sound: at most ${MOST_SAMPLES} samples, not ${samples.length}`);
  }
  return pcmBase64(samples);
}

function fade(fadeMs: number | undefined): Record<string, number> {
  return fadeMs === undefined ? {} : { fade_ms: wholeIn("fade_ms", fadeMs, 0, LONGEST_FADE_MS) };
}

function pageOf(page: number): string {
  if (!isPageNumber(page)) {
    throw new RangeError(`sound: ${page} is not a page number`);
  }
  return pageText(page);
}

function channelOf(channel: number): number {
  return wholeIn("channel", channel, 0, AUDIO_CHANNELS - 1);
}

/** The member as the protocol writes it, checked as the server checks it. */
export function soundFields(member: SoundMember): Record<string, unknown> {
  if ("object" in member) {
    const { object } = member;
    return { object: { ...handleFields(object), rate: rateOf(object.rate), pcm: samplesOf(object.samples) } };
  }
  if ("live" in member) {
    return { live: { ...handleFields(member.live), rate: rateOf(member.live.rate) } };
  }
  if ("blocks" in member) {
    const { blocks } = member;
    return {
      blocks: { ...handleFields(blocks), rate: rateOf(blocks.rate), first: wholeIn("first", blocks.first, 0, LAST_BLOCK), pcm: samplesOf(blocks.samples) },
    };
  }
  if ("play" in member) {
    const { play } = member;
    return {
      play: {
        channel: channelOf(play.channel),
        ...handleFields(play),
        ...(play.gain === undefined ? {} : { gain: wholeIn("gain", play.gain, 0, FULL_GAIN) }),
        ...fade(play.fadeMs),
        ...(play.loop === undefined ? {} : { loop: play.loop }),
        ...(play.after === undefined ? {} : { after: play.after }),
        ...(play.duck === undefined ? {} : { duck: wholeIn("duck", play.duck, 0, DEEPEST_DUCK) }),
        ...(play.page === undefined ? {} : { page: pageOf(play.page) }),
      },
    };
  }
  if ("stop" in member) {
    return { stop: { channel: channelOf(member.stop.channel), ...fade(member.stop.fadeMs) } };
  }
  const { gain } = member;
  return { gain: { channel: channelOf(gain.channel), gain: wholeIn("gain", gain.gain, 0, FULL_GAIN), ...fade(gain.fadeMs) } };
}
