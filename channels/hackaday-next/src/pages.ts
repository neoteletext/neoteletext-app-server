// Every page of the site (site.ts says where each is).

import type { PageSpec } from "../../../kits/typescript-components/src/index.ts";
import { homePages } from "./pages/home.ts";
import { podcastPages } from "./pages/podcast.ts";
import { readerPage } from "./pages/reader.ts";
import { sectionPages } from "./pages/sections.ts";
import { soundGamePages } from "./pages/sound-game.ts";
import type { Site } from "./parts.ts";

export function pages(site: Site): PageSpec[] {
  return [...homePages(site), readerPage(site), ...podcastPages(site), ...soundGamePages(site), ...sectionPages(site)];
}
