# The Hackaday channel's new hierarchy: requirements and work

The requirements and the list of work for reorganising the Hackaday channel around the whole of Hackaday's archive, as the owner set it out on 26 October 2026. Recorded as decision 11. The owner revised the brief the same afternoon (section 1.1): the reading is Claude Haiku 5.5's, not the local model's. The analysis is done, and the hierarchy it led to is `hackaday-hierarchy-design.md`, for the owner's review.

## 1. The owner's instructions

> "i want the hackaday dataset copied to foundintelquiet4060 in some new directory. i then want the llm on the machine to analyse the entire dataset but do not analyse the content of the comments and do not analsye the media records. i want you to research and find the logical content hierarchy/structure of the hackaday website and record that to a document. i then want YOU to drive that LLM so it does the analysis of all the hackday content - i want the content analysed and organised in all the ways that make sense, by author, by date, by topic, by tag i want to identify common themes and topics in the articles, create our own tag set. then bring in the understanding of how hackaday organises its own site. then come up with fun and funny new ideas for how to organise certain articles. it is ok for an article to appear in multiple placews in this new hierarchy. the hierarchy we have available to us MATCHES that of teletext"

The answers to the questions asked that day:

1. **Timing:** "do not prepare in advance - we are going to wait until the whisper project has finished completely before starting work."
2. **Where:** `~/hackaday-analysis/` on foundintelquiet4060.
3. **Ollama:** teletext-rag's Ollama may be restarted with settings for this work.
4. **New tools:** an embedding model and a clustering library may be used.
5. **Judgements:** "you are to make judgements": the local model reads, tags, names and brainstorms; Claude designs the prompts, checks samples, combines the results and chooses.
6. **Text:** the full text of every article.
7. **Limits:** the standard's maxima (`neoteletext-common/STANDARD-LIMITS.md`): 8 magazines, 2,022 pages (800 keyable), 8,190 sub-pages a page. And: "COMMENTS MAY NOT CONSUME PAGE OR SUB PAGE NUMBERS - COMMENTS WILL IN THE USER INTERFACE BE ATTACHED TO THE BOTTOM OF THE ARTICLE/PAGE TO WHICH THEY RELATE."
8. **Documents:** not answered; this document is in the server's `docs/`, where the channel's other documents are.
9. **Comments:** "NO DO NOT ANALYSE THE COMMENTS AT ALL": not their text, not their counts.

### 1.1 The revised brief, 26 October 2026

Given about 17:00, after the local model's reading had started; it replaces that reading, which is kept unused in the analysis database.

1. **The model:** Claude Haiku 5.5 through the Batches API for every step ("use haiku in batch mode"), thinking off. On 40 posts Haiku against Sonnet 5.5 priced the archive at $7.28 against $146 by batch.
2. **No quirks:** fun groupings come later, from the tags and themes across the archive.
3. **Tags:** Hackaday's own tags where they serve, merged for case, spelling and synonyms, and new ones only where they miss something. Two levels: tags on posts, and a few hundred subjects above them; each post has a main subject.
4. **The podcast:** every episode, each divided into its stories (leaving out the opening, the close, sponsors, listener mail and What's That Sound), each story with its transcript, its audio (16 kbit/s mono Opus) and the same metadata as a post, linked to the posts it discusses; the hosts as they introduce themselves.
5. **Speech:** faster-whisper removed from foundintelquiet4060; new episodes transcribed with Parakeet TDT 0.6B v2 through ONNX Runtime; the existing transcripts kept.
6. **Hackaday's structure:** "THE HACKADAY SITE STRUCTURE IS GOOD BUT NOT A REQUIREMENT - YOU ARE TRYING TO COME UP WITH A BETTER STRUCTURE".

## 2. Why the current hierarchy is not good enough

The current channel (`channels/hackaday/src/hackaday/site.py`, measured on the deployed commit `a4cf5df`: 392 pages, 3,015 sub-pages) was built in a day around a month of posts. The owner has judged it inadequate. These are the reasons the evidence gives.

1. **It is a window, not an archive.** It shows the latest 300 posts of 58,715 (0.5%), about one month of 22 years. The rest are reachable only one at a time, through search, roulette, "on this day" or the reader at 133. Nothing lets a viewer browse Hackaday's history.
2. **Page numbers do not stay put.** Stories fill slots N10 to N99 newest first, so every new post moves every older one down a page. The story on 215 today is on 216 tomorrow and gone in a month. Teletext's promise is that a number means a thing (Ceefax's 302 was always football); a bookmark or a link here goes stale in hours.
3. **Its subjects come from one month's rules, and the first rule wins.** A post goes to the first of eight magazines whose hand-written list of categories or tags it matches (`RULES`, written from 21 September to 25 October 2026). A post about a retro computer's security goes to Retro and never to Security; a post the rules miss goes to Making by default. The rules do not describe 2004 to 2025.
4. **The tag pages are noise.** Each subject magazine's six "busiest tags" are counted over its 30 to 50 stories, so most have 2 stories: "xhci (2)", "USB Driver (2)", "Windows 98 (2)", "llm-assisted project (2)". "ESP32" has a tag page in five magazines at once. Hackaday's own 57,022 tags are free text, unmerged ("ESP32", "esp32-C3", "ESP32-S3"), and the channel used them as they come.
5. **Magazine 1 is a cupboard.** It holds the front page, headlines, the magazines, two tag pages, search, talking points, figures, the columns, the people, the archive tools, the games, help and 30 story slots, numbered in no order a viewer could guess (113 is podcast sounds, 120 the people, 140 a quiz). The other magazines have the same template whatever their subject, and none uses more than 59 of its 100 numbers.
6. **Comments take numbers.** A story's comments are its sub-pages, 539 of the channel's 3,015 sub-pages. The owner has ruled that comments take no page or sub-page number.
7. **Hackaday's own structure is barely used.** Hackaday has series (This Week in Security, Hackaday Links, the Podcast, Retrotechtacular, Fail of the Week, Ask Hackaday, Hackaday Columns), contests (the Hackaday Prize, the challenges), events (Supercon, Hackaday Europe), 129 categories and 239 writers. The channel has three column pages, five people pages (the podcast's hosts), and categories only as inputs to its rules.
8. **There is no "by"**: no page by writer, by year, by month, by series, by theme. A teletext service is an index of its whole content.
9. **Our own limits shaped it:** 64 sub-pages a page and 800 decimal pages, where the standard allows 8,190 and 2,022.

## 3. The ideas to keep

These are the current channel's good ideas. Each is a requirement of the new hierarchy (section 4, requirement H.7).

1. **Magazines as subjects,** each with its own colour band, its name heading its pages in the contents, and the same template within it: N00 its front, N01 its feature, then its lists. Ceefax's lesson: a viewer learns one magazine and knows them all.
2. **A story is one page, its parts named sub-pages:** "Story 1/n" and on, "Picture", "Links", "Video n", named in row 1 so the browser lists them by name.
3. **Every lead picture as teletext:** the engine's converter makes each into a Level 2.5 mosaic page.
4. **QR codes** to the full article and each video, on the story's own sub-pages.
5. **The front page:** the Jolly Wrencher, the whole-page emblem for two seconds on arrival, the ticker, and the newsflash with its chime.
6. **Features with a pulse:** the Retrocomputing Challenge's countdown, This Week in Security, DOOM watch ("does it run DOOM").
7. **The people of Hackaday:** portraits in mosaics, each saying their own name in a podcast clip, their posts and their hackaday.io profile; the podcast soundboard.
8. **The archive as play:** Hack roulette with its spinning wheel, "On this day" across every year since 2004, "Then and now", the reader of any post.
9. **Interaction:** the quiz ("Guess the magazine"), the live poll, My Hackaday's favourites, search of the whole archive.
10. **Lists done properly:** one list component (keys, wheel, scroll bar, clicks), headlines a sub-page a day, tags A to Z, facts and figures (statistics, writers, behind the scenes), help in four parts.
11. **Navigation a remote can drive:** Fastext on every page (the magazine, its list, next, home), and the inserter's header with the live clock.

## 4. Requirements

The form is the server's (`requirements.md`): each has a statement, a reason, a source, a verification and a state.

### Requirement H.1: The hierarchy is teletext's, at the standard's maximum
- **Statement:** The new hierarchy must be magazines, pages and sub-pages only: 8 magazines; page numbers 00 to FE in each, less Annex A.10's reserved ones (2,022), of which the 800 decimal ones are those a viewer keys and the rest are reached by links; up to 8,190 sub-pages a page, numbered as `STANDARD-LIMITS.md` section 1 says. An article may appear in several places.
- **Reason:** The owner's instruction; the hierarchy "MATCHES that of teletext".
- **Source:** Decision 11; `neoteletext-common/STANDARD-LIMITS.md`.
- **Verification:** Check: the proposed hierarchy's every page number and sub-page count against `neoteletext_core::limits`.
- **State:** Not met, 26 October 2026. The channel can only be built to it once the work of `STANDARD-LIMITS.md` section 7 is done; until then the channel is bound to 64 sub-pages and 800 pages.

### Requirement H.2: Comments take no number
- **Statement:** A comment must never be a page or a sub-page, nor take a number of either. The browser shows an article's comments attached to the bottom of the article's page, in its own interface.
- **Reason:** The owner's instruction.
- **Source:** Decision 11.
- **Verification:** Check: no page or sub-page of the channel holds comments; test: the browser shows a story's comments under it.
- **State:** Not met, 26 October 2026: a story's comments are its sub-pages, 539 in all. **Needs a design:** comments are not teletext, so they need their own path from the channel to the browser. The proposal is a page's attachment, a list the channel publishes with the page and the browser draws beneath it as text, untrusted and bounded (owner to confirm, section 7).

### Requirement H.3: What is analysed
- **Statement:** The analysis must read every post's full text and fields (title, content, excerpt, date, modified date, writer, categories, tags, series, links), every category and tag, every writer and Hackaday's ten pages. It must not read comments in any way (not their text, authors or counts) and must not read media records.
- **Reason:** The owner's instructions.
- **Source:** Decision 11.
- **Verification:** Check: the analysis database's schema and every query of the pipeline; a test that its copy of the archive holds no `comments` or `media` rows.
- **State:** Met, 26 October 2026: the copy's tables are `posts`, `post_terms`, `terms` and `users` only (checked by hand; no automatic test); Haiku read the full text of the 58,716 posts and 10 pages.

### Requirement H.4: Where and when it runs
- **Statement:** The analysis must run on foundintelquiet4060 in `~/hackaday-analysis/`, on a consistent copy of the archive (SQLite `.backup` of the channel's `cache/hackaday.sqlite`, the comments and media tables then dropped from the copy), and must not start while the whatsthatsound project's run is still going (`pgrep -f "wts pipeline"` finds nothing and its log has ended).
- **Reason:** The owner's instructions; the GPU (an RTX 4060, 8 GB) cannot safely hold Whisper and the language model together, and a failed allocation would stop the transcription.
- **Source:** Decision 11.
- **Verification:** Check: the start script refuses while the whatsthatsound run is alive.
- **State:** Met, 26 October 2026: started after the Whisper run ended at 12:17 AEDT (`bin/start-check.sh`). Under the revised brief the reading ran on Anthropic's Batches API, from a copy of the analysis database; the host's copy is brought up to date from it.

### Requirement H.5: Haiku does the reading, Claude does the judging
- **Statement:** Claude Haiku 5.5, through the Batches API, must do the per-post reading (a summary, its kind, topics, themes, which of its Hackaday tags apply, new tags), the merging of tags, the slicing of the podcast and the choice of each post's subjects. Claude must write and test the prompts, check samples of every step, combine the results, curate the subjects and design the hierarchy. Every model answer is structured (a JSON schema), kept with its prompt's version, and re-runnable.
- **Reason:** The owner's instructions: "use haiku in batch mode" (section 1.1, replacing "i want the local LLM doing the work, driven by you"), "you are to make judgements".
- **Source:** Decision 11.
- **Verification:** Check: each step's sample review recorded in the analysis log.
- **State:** Met, 26 October 2026, except that the sample reviews are recorded in `hackaday-hierarchy-design.md` section 1 and the prompts' versions, not yet in the analysis log.

### Requirement H.6: What comes out
- **Statement:** The work must produce:
  1. **Hackaday's own structure**, researched and written down: its categories and their parents, its series and columns, contests and events, its writers, its menus and archives, how it uses tags (`hackaday-site-structure.md`).
  2. **The archive organised** by writer, by date (year, month, day of the week, time of day), by Hackaday category, by Hackaday tag (merged: case, spelling and synonyms), by series, and by topic and theme found in the text.
  3. **Our own tag set:** a few hundred tags, each with a definition, its Hackaday tags and categories, and its articles; every article tagged.
  4. **Common themes and topics:** clusters of articles, named, with their spread over the years.
  5. **Fun and funny ways to organise articles** (in the spirit of DOOM watch: "things that run DOOM", "hacks that caught fire", "the same idea, twenty years apart"), each with its articles.
  6. **The new hierarchy:** every magazine, page and sub-page with its number, its title and what fills it, built to requirements H.1, H.2 and H.7.
- **Reason:** The owner's instructions.
- **Source:** Decision 11.
- **Verification:** Review by the owner; checks of the hierarchy against H.1.
- **State:** Partly met, 26 October 2026: item 1 is `hackaday-site-structure.md`; items 2 and 3 are in the analysis database (tags, subjects, writers, dates, series), with subjects of a sentence's scope in place of tags with definitions (the revised brief's two levels); item 4's spread over the years is not done; item 5 is deferred by the revised brief; item 6 is `hackaday-hierarchy-design.md`, a draft for review.

### Requirement H.7: The good ideas carried forward
- **Statement:** The new hierarchy must keep each idea of section 3, and must fix each fault of section 2: an archive, not a window; numbers that stay put; subjects from the analysis, an article in every place it belongs; tags merged and meaningful; magazine 1 ordered; comments without numbers; Hackaday's own structure and every "by".
- **Reason:** The owner: "it does have some great ideas in it so ensure those great ideas come into the new hierarchy".
- **Source:** Decision 11.
- **Verification:** Review: a table of section 3's ideas and section 2's faults, each with where the new hierarchy keeps or fixes it.
- **State:** Drafted, 26 October 2026: `hackaday-hierarchy-design.md` section 10, for the owner's review.

## 5. Tools

- **The language model:** Llama 3.1 8B (Q4_K_M) under teletext-rag's Ollama 0.40.5, restarted for the work with four requests at once, flash attention and a quantised key-value cache (the owner's answer 3), and restored after.
- **Embeddings:** a small embedding model through the same Ollama (nomic-embed-text, about 270 MB), for finding topics by similarity (the owner's answer 4).
- **Clustering:** scikit-learn, in a uv project of its own in `~/hackaday-analysis/`, its versions locked (the owner's answer 4; approved by name in decision 11).
- **The analysis's own database:** SQLite, STRICT tables, numbered migrations, every model answer kept as JSON with its prompt version (the owner's rules for SQLite).

## 6. The work

In order, once the Whisper run has finished. Each step's sample is checked before the next starts.

1. **Confirm the start:** the whatsthatsound run has ended; the GPU's memory is free.
2. **Copy:** `.backup` of the archive to `~/hackaday-analysis/hackaday.sqlite`; drop comments and media from the copy; record the counts.
3. **Restart Ollama** with the work's settings; pull the embedding model; measure throughput on 100 articles (input and output tokens a second) and give the owner the time the full run will take.
4. **Hackaday's structure:** the categories' tree, series by title pattern and tag, contests and events, writers and their spans, from the copy and from Hackaday's public pages; write `hackaday-site-structure.md`.
5. **The plain organisations** (no model): by writer, by date, by category, by tag with tags merged, by series; their tables in the analysis database.
6. **Per-article reading:** every article's full text through the model with one prompt: topics, themes, proposed tags, a summary line; structured, stored, resumable.
7. **Embeddings** of every article; clusters; the model names each cluster from samples of its articles.
8. **Our tag set:** Claude merges the proposed tags, the clusters and Hackaday's tags into one set with definitions; the model tags every article against it; Claude checks samples.
9. **Themes over time:** each theme's spread by year and by writer.
10. **Fun ideas:** the model brainstorms groupings from the themes and tags; Claude chooses and the model fills each with its articles.
11. **The hierarchy:** Claude designs it to H.1, H.2 and H.7, with the model's help naming pages; written into this document's sequel with every number.
12. **Restore Ollama's settings;** report to the owner.
13. **Build:** the channel to the new hierarchy, after `STANDARD-LIMITS.md` section 7 and the comments design of H.2.

## 7. Questions for the owner

1. **Comments in the browser (H.2):** a list the channel publishes with each page, which the browser draws as text beneath the page, scrolling with it. Is that what you mean by "attached to the bottom of the article/page"?
2. **Where the results live:** the analysis's database and outputs in `~/hackaday-analysis/` on foundintelquiet4060, and its documents (this one, `hackaday-site-structure.md`, the hierarchy) in the server's `docs/`?
