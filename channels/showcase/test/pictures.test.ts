import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { loadPictures } from "../src/pictures.ts";

test("pictures are read from captions.tsv and each level's TTI file", () => {
  const folder = mkdtempSync(join(tmpdir(), "showcase-pictures-"));
  try {
    writeFileSync(
      join(folder, "captions.tsv"),
      "e01\te\tA cat · Ada · CC BY 4.0\ns01\ts\tA ship\tin two parts\nmissing\ta\tNo files\n../up\ta\tA path\nshort\n",
    );
    writeFileSync(join(folder, "e01_l15.tti"), "PN,10000\nOL,1,cat\n");
    writeFileSync(join(folder, "e01_l35.tti"), "PN,10000\nOL,1,CAT\n");
    writeFileSync(join(folder, "s01_l25.tti"), "PN,10000\n");
    mkdirSync(join(folder, "up_l15.tti"));
    const pictures = loadPictures(folder);
    assert.deepEqual(
      pictures.map(({ name, group, caption, levels }) => [name, group, caption, levels.map(({ level }) => level)]),
      [
        ["e01", "e", "A cat · Ada · CC BY 4.0", ["l15", "l35"]],
        ["s01", "s", "A ship\tin two parts", ["l25"]],
      ],
    );
    assert.equal(pictures[0]?.levels[0]?.tti, "PN,10000\nOL,1,cat\n");
  } finally {
    rmSync(folder, { recursive: true });
  }
  assert.deepEqual(loadPictures(join(folder, "gone")), [], "no folder, no pictures");
});
