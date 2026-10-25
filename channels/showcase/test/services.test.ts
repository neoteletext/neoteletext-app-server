// crates/teletext-showcase/src/services/tests.rs, ported: the live
// services driven as the server drives them.

import assert from "node:assert/strict";
import { test } from "node:test";

import { Key } from "../../../kits/typescript/src/kit.ts";
import { Harness, type Sent } from "../../../kits/typescript/src/testing.ts";
import { INDEX } from "../src/common.ts";
import { services } from "../src/services.ts";
import { aboutScreens, factsScreens } from "../src/services/about.ts";
import { CivilDate, DAY_MS, SECOND_MS, hoursMinutes, hoursMinutesSeconds, local } from "../src/services/calendar.ts";
import { wrap } from "../src/services/draw.ts";
import { onNow, screen as guideScreen } from "../src/services/guide.ts";
import { Instrument, STEP_CHOICES } from "../src/services/markets.ts";
import { screens as newsScreens } from "../src/services/news.ts";
import {
  ABOUT,
  CLOCK,
  FACTS,
  GUIDE,
  LONG_READ,
  MARKETS,
  NEWS,
  SERVICES,
  SPORT,
  WEATHER,
  WORLD_CLOCK,
  navigation,
} from "../src/services/pages.ts";
import { Standing, fixtures } from "../src/services/sport.ts";
import { forecast, temperature } from "../src/services/weather.ts";
import { screen as worldClockScreen } from "../src/services/world-clock.ts";

/** Midnight UTC starting 25 October 2026. */
const MIDNIGHT = 1_792_886_400_000;
/** 25 October 2026, 14:03:22 UTC. */
const SOME_TIME = MIDNIGHT + (14 * 3_600 + 3 * 60 + 22) * 1_000;
const HOUR_MS = 3_600 * 1_000;

function channel(epochMs: number): Harness {
  return new Harness(services.applications([]), { epochMs });
}

/** The latest shared TTI of each page among `lines`. */
function shared(lines: readonly Sent[]): Map<number, string> {
  const pages = new Map<number, string>();
  for (const line of lines) {
    if (line["type"] === "publish" && line["owner"] === "channel") {
      pages.set(Number.parseInt(String(line["page"]), 16), String(line["tti"]));
    }
  }
  return pages;
}

test("every page is published at start, and shared", () => {
  const harness = channel(SOME_TIME);
  const expected = new Set(SERVICES.map(([page]) => page));
  assert.deepEqual(new Set(shared(harness.sent).keys()), expected);
  assert.deepEqual(new Set(harness.channel.entries().map((entry) => entry.page)), expected);
  assert.deepEqual(harness.errors, []);
});

test("live pages change and fixed pages do not", () => {
  const harness = channel(SOME_TIME);
  const first = shared(harness.sent);
  const before = harness.sent.length;
  harness.runUntil(61_000);
  const later = shared(harness.sent.slice(before));
  // The clocks change each second, the weather and the guide at the
  // minute, the markets every 3 s and the sport every 15 s.
  for (const page of [WEATHER, WORLD_CLOCK, MARKETS, CLOCK, SPORT, GUIDE]) {
    assert.ok(later.has(page), `${page.toString(16)} republished`);
    assert.notEqual(later.get(page), first.get(page), page.toString(16));
  }
  // The carousels never change, so they are never sent again.
  for (const page of [NEWS, ABOUT, FACTS]) {
    assert.ok(!later.has(page), `${page.toString(16)} republished`);
  }
});

test("the clocks change after their deadline", () => {
  const harness = channel(SOME_TIME);
  const wake = harness.channel.nextWake();
  assert.ok(wake !== null && wake <= SECOND_MS, String(wake));
  const before = harness.sent.length;
  harness.clock.time = wake;
  harness.channel.deadline();
  assert.deepEqual(new Set(shared(harness.sent.slice(before)).keys()), new Set([WORLD_CLOCK, CLOCK]));
});

test("a long run keeps publishing", () => {
  // Three hours from two hours before midnight UTC: several sport rounds,
  // and the day changing for the guide and the weather.
  const harness = channel(MIDNIGHT - 2 * HOUR_MS);
  const before = harness.sent.length;
  const until = 3 * HOUR_MS;
  harness.runUntil(until);
  const sport = harness.sent.slice(before).filter((line) => line["type"] === "publish" && line["page"] === "106").length;
  assert.equal(sport, (3 * 3_600) / 15);
  const next = harness.channel.nextWake();
  assert.ok(next !== null && next > until);
  assert.deepEqual(harness.errors, []);
});

test("civil dates are right", () => {
  const cases: [number, [number, number, number, string]][] = [
    [0, [1970, 1, 1, "Thursday"]],
    [951_782_400_000, [2000, 2, 29, "Tuesday"]],
    [1_792_886_400_000, [2026, 10, 25, "Sunday"]],
    [4_107_542_400_000, [2100, 3, 1, "Monday"]],
    [94_608_000_000, [1972, 12, 31, "Sunday"]],
    [13_574_563_200_000, [2400, 2, 29, "Tuesday"]],
  ];
  for (const [epochMs, [year, month, day, weekday]] of cases) {
    const date = CivilDate.fromEpochMs(epochMs);
    assert.deepEqual([date.year, date.month, date.day, date.weekdayName()], [year, month, day, weekday], String(epochMs));
    if (epochMs > 0) {
      assert.notEqual(CivilDate.fromEpochMs(epochMs - 1).day, day, String(epochMs));
    }
  }
  assert.equal(CivilDate.fromEpochMs(SOME_TIME).long(), "Sunday 25 October 2026");
  assert.equal(CivilDate.fromEpochMs(SOME_TIME).short(), "Sun 25 Oct");
  assert.equal(hoursMinutesSeconds(SOME_TIME), "14:03:22");
  assert.equal(Math.floor(SOME_TIME / DAY_MS), 20_751);
});

test("every day follows the one before", () => {
  // 450 years of days, through the leap years and the century rules, each
  // one day on from the last.
  let last = CivilDate.fromDays(0);
  for (let days = 1; days < 450 * 366; days += 1) {
    const date = CivilDate.fromDays(days);
    const nextDay = date.day === last.day + 1 && date.month === last.month && date.year === last.year;
    const nextMonth = date.day === 1 && date.month === last.month + 1 && date.year === last.year;
    const nextYear = date.day === 1 && date.month === 1 && last.month === 12 && last.day === 31 && date.year === last.year + 1;
    assert.ok(nextDay || nextMonth || nextYear, `${JSON.stringify(last)} then ${JSON.stringify(date)}`);
    assert.equal(date.weekday, (last.weekday + 1) % 7);
    last = date;
  }
});

test("local times move by their offset", () => {
  assert.equal(hoursMinutes(local(SOME_TIME, 540)), "23:03");
  assert.equal(hoursMinutes(local(SOME_TIME, 330)), "19:33");
  assert.equal(hoursMinutes(local(SOME_TIME, -600)), "04:03");
  // Behind UTC at its start, the clock stops at the epoch.
  assert.equal(local(0, -60), 0);
});

test("the world clock shows each city", () => {
  const screen = worldClockScreen(SOME_TIME);
  const rows = Array.from({ length: 25 }, (_, row) => screen.rowText(row));
  const all = rows.join("");
  for (const expected of ["Tokyo", "23:03", "Honolulu", "04:03", "Sunday 25 October 2026", "daylight saving"]) {
    assert.ok(all.includes(expected), expected);
  }
  // Sydney is past midnight: Monday.
  const sydney = rows.find((row) => row.includes("Sydney")) ?? "";
  assert.ok(sydney.includes("00:03") && sydney.includes("Mon"), sydney);
});

test("market walks stay bounded", () => {
  const instruments = [new Instrument("up", 10_000), new Instrument("down", 10_000), new Instrument("random", 10_000)];
  const bits = 64;
  let seed = 0x1234_5678n;
  for (let step = 0; step < 100_000; step += 1) {
    seed = BigInt.asUintN(bits, seed ^ (seed << 13n));
    seed ^= seed >> 7n;
    seed = BigInt.asUintN(bits, seed ^ (seed << 17n));
    const random = Number(seed % BigInt(STEP_CHOICES));
    [STEP_CHOICES - 1, 0, random].forEach((choice, index) => {
      const instrument = instruments[index];
      assert.ok(instrument !== undefined);
      instrument.walk(choice);
      assert.ok(instrument.value >= instrument.floor() && instrument.value <= instrument.ceiling(), JSON.stringify(instrument));
    });
  }
  const [up, down] = instruments;
  assert.ok(up !== undefined && down !== undefined);
  // The pull back stops even a steady rise or fall short of the bounds.
  assert.ok(up.value < up.ceiling() && up.value > up.base);
  assert.ok(down.value > down.floor() && down.value < down.base);
});

test("every team plays each round and meets each other home and away", () => {
  const meetings = new Map<string, number>();
  for (let round = 0; round < 14; round += 1) {
    const matches = fixtures(round);
    const teams = new Set(matches.flatMap((game) => [game.home, game.away]));
    assert.deepEqual(teams, new Set([0, 1, 2, 3, 4, 5, 6, 7]), `round ${round}`);
    for (const game of matches) {
      const pair = `${game.home}-${game.away}`;
      meetings.set(pair, (meetings.get(pair) ?? 0) + 1);
    }
  }
  const expected = new Map<string, number>();
  for (let home = 0; home < 8; home += 1) {
    for (let away = 0; away < 8; away += 1) {
      if (away !== home) {
        expected.set(`${home}-${away}`, 1);
      }
    }
  }
  assert.deepEqual(meetings, expected);
});

test("standings count results", () => {
  const standing = new Standing().with(3, 1).with(0, 0).with(1, 2);
  assert.deepEqual([standing.played, standing.won, standing.drawn, standing.lost], [3, 1, 1, 1]);
  assert.deepEqual([standing.points(), standing.difference()], [4, 1]);
});

test("the weather follows the day", () => {
  const day = forecast(20_000, 0);
  assert.ok(day.low < day.high);
  assert.equal(temperature(day, 300), day.low);
  assert.equal(temperature(day, 900), day.high);
  let last = day.low;
  for (let minute = 300; minute <= 900; minute += 1) {
    const now = temperature(day, minute);
    assert.ok(now >= last && now <= day.high, String(minute));
    last = now;
  }
  for (let minute = 901; minute < 1_440 + 300; minute += 1) {
    const now = temperature(day, minute % 1_440);
    assert.ok(now <= last && now >= day.low, String(minute));
    last = now;
  }
  // Every viewer, and every restart, sees the same day.
  assert.deepEqual(forecast(20_000, 3), forecast(20_000, 3));
});

test("the guide finds the programme on now", () => {
  assert.equal(onNow(0), 0);
  assert.equal(onNow(359), 0);
  assert.equal(onNow(360), 1);
  assert.equal(onNow(1_439), 19);
  const screen = guideScreen(SOME_TIME);
  // 14:03 is in the afternoon film, from 13:30 to 15:30.
  assert.ok(screen.rowText(5).includes("13:30") && screen.rowText(5).includes("The Lost Sub-page"), screen.rowText(5));
  assert.ok(screen.rowText(7).includes("Ends 15:30, 1 h 27 min to go"), screen.rowText(7));
});

test("navigation links three other services and the index", () => {
  for (const [page] of SERVICES) {
    const links = navigation(page);
    const pages = new Set(links.map(([, link]) => link).filter((link) => link !== null));
    assert.equal(pages.size, 4, page.toString(16));
    assert.ok(!pages.has(page), page.toString(16));
    assert.ok(pages.has(INDEX));
    assert.ok(links.every(([label]) => label.length > 0 && label.length <= 9));
  }
});

test("text wraps at spaces", () => {
  assert.deepEqual(wrap("one two three four", 9), ["one two", "three", "four"]);
  assert.deepEqual(wrap("abcdefghijk", 4), ["abcd"]);
  assert.deepEqual(wrap("", 10), []);
});

test("carousels number their sub-pages", () => {
  for (const [screens, count] of [
    [newsScreens(), 7],
    [aboutScreens(), 4],
    [factsScreens(), 9],
  ] as const) {
    assert.equal(screens.length, count);
    screens.forEach((screen, index) => {
      assert.ok(screen.rowText(1).trimEnd().endsWith(`${index + 1}/${count}`), screen.rowText(1));
    });
  }
});

test("a carousel is published as sub-pages 1 on, with its Fastext row shown", () => {
  const harness = channel(SOME_TIME);
  const pages = harness.sharedPages(NEWS);
  assert.deepEqual(
    pages.map((page) => page.subpage),
    [1, 2, 3, 4, 5, 6, 7],
  );
  assert.ok(pages.every((page) => page.fastextRow));
  assert.deepEqual(harness.sharedPages(WEATHER).map((page) => page.subpage), [0]);
});

// Not in the Rust showcase, whose service pages ignored digits and the
// coloured keys while keys went to the channel (INPUT.md section 9.4).
test("digits and the coloured keys select pages from the services", () => {
  const harness = channel(SOME_TIME);
  harness.join(1, NEWS);
  harness.typeText(1, "1");
  harness.typeText(1, "0");
  harness.typeText(1, "9");
  assert.deepEqual(harness.gotos(1), [FACTS]);
  harness.press(1, Key.F1);
  harness.press(1, Key.Green);
  harness.press(1, Key.Blue);
  assert.deepEqual(harness.gotos(1), [FACTS, LONG_READ, WEATHER, INDEX], "red the service before, green after, cyan the index");
  harness.typeText(1, "999");
  assert.deepEqual(harness.gotos(1), [FACTS, LONG_READ, WEATHER, INDEX], "no magazine 9");
});
