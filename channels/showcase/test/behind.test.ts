// Page 150, behind the scenes: the server's figures, as the server sends
// them every 10 seconds (docs/applications.md section 7).

import assert from "node:assert/strict";
import { test } from "node:test";

import type { ServerStats } from "../../../kits/typescript/src/kit.ts";
import { Harness, rowOf, shows, text } from "../../../kits/typescript/src/testing.ts";
import { applications } from "../src/areas.ts";
import { INDEX } from "../src/common.ts";
import { services } from "../src/services.ts";
import { MOST_OPENED_SHOWN, TITLE, amount, fit } from "../src/services/behind.ts";
import { ABOUT, BEHIND, LONG_READ, NEWS } from "../src/services/pages.ts";

/** Figures as the server writes them: Hackaday's channel beside the showcase, the server on a system that gives no memory figure. */
const STATS: ServerStats = JSON.parse(
  JSON.stringify({
    server: {
      uptime_s: 3_725,
      connections: 3,
      connections_peak: 5,
      connections_total: 40,
      messages_sent: 1_234,
      bytes_sent: 1_572_864,
      requests: 90,
      not_modified: 12,
      restarts: 0,
      memory_kb: -1,
      channels: 2,
      viewers: { hackaday: 2, showcase: 1 },
    },
    channel: {
      viewers: 1,
      viewers_peak: 4,
      viewers_total: 31,
      opens: 77,
      inputs: 50,
      keys: 40,
      shared_published: 900,
      own_published: 120,
      gotos: 6,
      sound_records: 0,
      lines_to_application: 200,
      lines_from_application: 1_100,
      most_opened: { "110": 9, "100": 30, "150": 9, "1A0": 2, "101": 5, "108": 4, "105": 3, "999": 1 },
    },
  }),
) as ServerStats;

test("the page waits for the server's figures, then shows them", () => {
  const harness = new Harness(services.applications([]));
  assert.ok(shows(harness.shared(BEHIND), "Waiting for the server's figures"));
  harness.stats(STATS);
  const rows = harness.shared(BEHIND);
  assert.ok(shows(rows, "WATCHING") && shows(rows, "THE SHOWCASE NOW"));
  assert.ok(shows(rows, "Peak 4, 31 visits since start"));
  assert.ok(shows(rows, "Pages opened 77, keys 40"));
  assert.ok(shows(rows, "Pages drawn 900 shared, 120 own"));
  assert.ok(shows(rows, "Sent 1.5 MB in 1234 messages"));
  assert.ok(shows(rows, "Server up 1h 02m, memory n/a"), "the server's -1 is no figure");
  assert.ok(shows(rows, "Watching: hackaday 2, showcase 1"));
  assert.deepEqual(harness.errors, []);
});

test("the most opened pages are listed the most first, with their titles", () => {
  const harness = new Harness(services.applications([]));
  harness.stats(STATS);
  const rows = harness.shared(BEHIND);
  const first = rowOf(rows, "Most opened pages");
  assert.ok(first !== undefined);
  const listed = Array.from({ length: MOST_OPENED_SHOWN }, (_, offset) => text(rows, first + 1 + offset));
  // 100 is no page of the services', so it has no title here; 999 is no page at all.
  assert.deepEqual(
    listed.map((row) => row.slice(1, 4)),
    ["100", "110", "150", "101", "108", "105"],
  );
  assert.ok(listed[1]?.includes("The long read") && listed[1].trimEnd().endsWith("9"), listed[1]);
  assert.ok(listed[2]?.includes(TITLE));
  assert.ok(!shows(rows, "1A0"), "beyond the six shown");
});

test("the whole channel's titles name its pages", () => {
  const harness = new Harness(applications());
  harness.stats(STATS);
  const rows = harness.shared(BEHIND);
  const index = rowOf(rows, "100");
  assert.ok(index !== undefined && text(rows, index).includes("Index"), "the index is another area's page");
});

test("each new set of figures draws the page again for every viewer", () => {
  const harness = new Harness(services.applications([]));
  harness.join(1, BEHIND);
  harness.sharedPublications(BEHIND);
  harness.stats(STATS);
  harness.stats({ ...STATS, channel: { ...STATS.channel, viewers: 2 } });
  assert.equal(harness.sharedPublications(BEHIND), 2);
  assert.equal(harness.ownPages(1, BEHIND), undefined, "the channel's page, never a viewer's own");
});

test("the page is on the index and in the services' ring", () => {
  const harness = new Harness(applications());
  const index = harness.shared(INDEX);
  const row = rowOf(index, "Behind the scenes");
  assert.ok(row !== undefined && text(index, row).includes("150"));
  assert.deepEqual(harness.sharedPages(BEHIND)[0]?.links.slice(0, 4), [LONG_READ, NEWS, ABOUT, INDEX], "red the long read, green round to the news");
  assert.deepEqual(harness.sharedPages(LONG_READ)[0]?.links[1], BEHIND, "the long read's green");
  assert.equal(harness.title(BEHIND), TITLE);
});

test("figures are said as a reader says them, and cut to fit", () => {
  assert.deepEqual([0, 1023, 1024, 1_572_864, 3 * 1024 ** 3].map(amount), ["0 B", "1023 B", "1.0 KB", "1.5 MB", "3.0 GB"]);
  assert.equal(fit("short", 10), "short");
  assert.equal(fit("a longer line of text", 10), "a longer..", "ten characters, the mark among them");
});
