# How Hackaday organises its own site

How hackaday.com structures its 22 years of posts: its categories, series, contests and events, writers, menus, archives and tags, measured from the archive and from the site's public pages on 26 October 2026. It is an input to the Hackaday channel's new hierarchy (`hackaday-hierarchy.md`, decision 11). It describes Hackaday's structure; it does not design ours, except for the judgement in section 10.

## 1. Sources and method

**The archive.** A SQLite copy at `foundintelquiet4060:~/hackaday-analysis/analysis.sqlite`, opened read-only (`sqlite3 -readonly`). It holds 58,716 posts (5 September 2004 to 25 October 2026), 10 pages, 129 categories, 57,022 tags and 239 writers. Every number in this document comes from a query against it; the key queries are in Appendix A. Comments and media records were not read, counted or looked for.

Three properties of the data matter when reading the numbers:

- **Categories and tags are as Hackaday has them today, not as they were when each post was written.** Hackaday has refiled old posts into newer categories, so a category's first post can be years older than the category (Retrocomputing's first post is from 2005; it had fewer than 12 posts a year until 2018). The tables therefore give both the first post and the span of years with at least 12 posts, which is closer to when a category was really in use.
- **Times are GMT.** Hackaday schedules in US Pacific time, so a post on Sunday evening in California is dated Monday here.
- **Hackaday's own counts** (`terms.post_count`) agree with the copy's for every category but two, which differ by one (Hackaday Columns, Hackaday links), and for all but 504 of the 57,022 tags; the copy holds 280,212 tag links against Hackaday's 279,689. The counts here are the copy's.

**The public pages.** 14 pages fetched on 26 October 2026, each linked from the site's navigation, the front page, or a post on it, a few seconds apart, without logging in or submitting anything:

| Page | URL |
|---|---|
| Front page | https://hackaday.com/ |
| Blog | https://hackaday.com/blog/ |
| About | https://hackaday.com/about/ |
| Hackaday Columns category | https://hackaday.com/category/hackaday-columns/ |
| Featured category | https://hackaday.com/category/featured/ |
| Slider category | https://hackaday.com/category/slider/ |
| The Hackaday Prize category | https://hackaday.com/category/the-hackaday-prize/ |
| A tag page | https://hackaday.com/tag/hackaday-podcast/ |
| A day archive | https://hackaday.com/2026/10/25/ |
| An author page | https://hackaday.com/author/aaronbeckendorf/ |
| A Supercon post | https://hackaday.com/2026/10/14/2026-hackaday-supercon-workshops-are-open/ |
| A podcast post | https://hackaday.com/2026/10/23/hackaday-podcast-episode-392-microscopes-drones-and-other-diy-things/ |
| Hackaday.io contests (menu "Contests") | https://hackaday.io/contests |
| Hackaday.io front page (menu "Hackaday.io") | https://hackaday.io/ |

## 2. Menus and archives: how the site's own navigation works

Hackaday's navigation is thin. A visitor reaches posts through the front page's blocks, the reverse-chronological blog, a flat category list, and the category, tag, author and date links on each post. There is no menu of sections.

**The primary menu** (https://hackaday.com/) has six items: Home, Blog, Hackaday.io, Contests (which goes to hackaday.io/contests), Submit (the tip form) and About, plus a search box. The footer menu is Home, Blog, Hackaday.io, Video (the YouTube channel), Submit A Tip, About and Contact Us. Neither menu names a subject, a series or an event.

**The front page** is a set of blocks, and these are the closest thing the site has to sections:

| Block | What fills it | Source |
|---|---|---|
| Lead stories (the slider, 5 stories) | posts in the category **Slider** | front page; https://hackaday.com/category/slider/ |
| "From the Blog" (7 stories, "See all blog entries") | the latest posts of every kind | front page; https://hackaday.com/blog/ |
| "If you missed it" | posts in **Featured**: "Longform articles, the best of what the Hackaday writing crew has to offer." | https://hackaday.com/category/featured/ |
| "Our Columns" | posts in **Hackaday Columns** | https://hackaday.com/category/hackaday-columns/ |
| "Featured Projects", "Trending Projects" | projects on hackaday.io, not posts | front page |

The sidebar has search, a newsletter sign-up, recent comments, "Now on Hackaday.io", and a **"Select Category" dropdown** listing all 129 categories alphabetically, with no nesting and no counts (https://hackaday.com/blog/). There is no tag cloud and no month or year archive widget.

**Archive pages.** Every list on the site is the same WordPress archive: seven posts a page, newest first, each with picture, title, date, author, comment count, excerpt, "Posted in" (categories) and "Tagged" (tags), and a single "← Older posts" link to `/page/N/`. There are no page numbers.

| Archive | URL form | Heading | Example |
|---|---|---|---|
| Blog | `/blog/`, `/blog/page/N/` | none | https://hackaday.com/blog/ |
| Category | `/category/<slug>/` | name, "N Articles", the description if any | https://hackaday.com/category/featured/ ("3163 Articles") |
| Tag | `/tag/<slug>/` | name, "N Articles" | https://hackaday.com/tag/hackaday-podcast/ ("395 Articles") |
| Author | `/author/<login>/` | "Author: name", "N Articles", no biography on the page | https://hackaday.com/author/aaronbeckendorf/ ("192 Articles") |
| Day | `/YYYY/MM/DD/` | "Day: October 25, 2026" | https://hackaday.com/2026/10/25/ |

A post's date links to its day's archive; nothing on the pages fetched links to a month or year archive (none was probed). A post's own address is `/YYYY/MM/DD/slug/`, and its page links the previous and next posts in time, not in its series (https://hackaday.com/2026/10/23/hackaday-podcast-episode-392-microscopes-drones-and-other-diy-things/). The only feed linked is the whole site's, `https://hackaday.com/blog/feed/`.

**Pages.** The archive's 10 pages are the site's furniture, not content: Home, Blog, About Hackaday, Contact us, Submit a tip, Policies, Digital Services Act, Prize (2020), HaDuino (2013) and Hackaday Retro Edition (2021).

**Hackaday.io** is the sister site, "the world's largest collaborative hardware development community" (https://hackaday.io/): readers' projects, pages, lists, the Hack Chat, hackerspaces and the contests. hackaday.com links to it for readers' projects and for the contests, whose entries are hackaday.io projects; the contests page lists 21 contests, newest first, with no dates (https://hackaday.io/contests).

## 3. Categories

### 3.1 The tree

The 129 categories are almost flat. Only two have children:

- **Hackaday Columns** (4,892 posts): Retrotechtacular (304), Ask Hackaday (189), Fail of the Week (134). Most columns are not children of it but are marked by title (section 4).
- **Curated** (76 posts of its own, 2016 to 2023): Original Art (1,271), Interest (1,044), History (640), Current Events (632), Engineering (614), Skills (367), Business (73), Fiction (16). Its description is "Hackaday's finest original content articles curated by the Hackaday editors"; these are the genres of the staff's own writing as opposed to write-ups of readers' hacks.

The other 116 neither have a parent nor are one. The site shows none of this nesting: its dropdown is a flat alphabetical list.

A post carries 1.73 categories on average: 30,590 posts have one, 19,029 two, 5,509 three and 3,588 four or more (one has 12). None has none, but 356 are "Uncategorized", most from 2005 to 2008.

### 3.2 Kinds of category

Read against their contents, the 129 fall into kinds that should be kept apart:

| Kind | Count | Examples | What it means |
|---|--:|---|---|
| Subject | 67 | Misc Hacks, Tool Hacks, 3d Printer hacks, Robots Hacks, Security Hacks, Radio Hacks, Retrocomputing, Science, Space | what a post is about; the usual name form is "X Hacks" |
| Platform | 5 | Arduino Hacks, Raspberry Pi, FPGA, ARM, ATtiny Hacks | the board or chip used; Arduino and Raspberry Pi are among the largest categories |
| Format | 10 | News, Rants, Reviews, Interviews, how-to, Teardown, Biography, Parts | the form of the post, not its subject |
| Original-content genre | 8 | the children of Curated | the staff's own articles, from 2015 |
| Series | 10 | Hackaday Columns, Hackaday links, Retrotechtacular, Ask Hackaday, Fail of the Week, Podcasts, HackIt, Roundup | recurring columns (section 4) |
| Contest or event | 6 | The Hackaday Prize, contests, cons, Hackerspaces, Crowd Funding, Hackaday Store | section 5 |
| Display flag | 3 | Slider, Featured, Curated | where a post is shown, not what it is |
| Tag in disguise | 20 | g1 hacks (26 posts, 2008–2009), Palm Pre Hacks (6), Nook Hacks (3), firefox hacks (24), Kinect hacks, Netbook Hacks, PSP Hacks, Nintendo DS Hacks, Wii, Xbox, Playstation, iphone, ipod, blackberry, Kindle, google hacks | one product, mostly of 2005 to 2012, each now a handful of posts a year or none |

The two largest categories are display flags: **Slider** (5,162 posts, 2014 on; the front page's lead stories) and **Featured** (3,163; the "If you missed it" block). 2,085 posts are in both, and 1,972 Slider posts are columns. Neither says what a post is about.

**The categories have three eras.** The categories of 2004 to 2012 are hardware of the day ("ipod hacks", "Netbook Hacks", "g1 hacks", "Kinect hacks", "downloads hacks"), most of them dead now. From 2013 to 2016 Hackaday added the editorial structure: Hackaday Columns (2013), Slider (2014), The Hackaday Prize (2014), Curated and its genres (2015). From 2016 to 2023 it added broad subjects: Science (2016), Space (2017), Retrocomputing (in use from 2018), Games (2018), Artificial Intelligence, Reverse Engineering and Battery Hacks (2022), PCB Hacks (2023). Several broad old categories have faded: Misc Hacks had 456 posts in 2008 and 44 in 2025 and 2026 together; Arduino Hacks had 380 in 2011 and 52 since 2025.

**Categories are mirrored by tags**, which compete with them: the category Arduino Hacks (3,095 posts) and the tag "arduino" (2,951); Raspberry Pi (1,957) and "raspberry pi" (1,748); Retrocomputing (1,700) and "retrocomputing" (398); Hackaday links (820) and the tag "hackaday links" (354).

### 3.3 Every category

Posts in the archive; the first year with a post; the last post; the span of years with at least 12 posts (the category's real period of use); posts since 1 January 2025; and the kind from section 3.2.

| Category | Parent | Posts | First | Last post | Years with 12+ posts | Posts 2025–26 | Kind |
|---|---|--:|---|---|---|--:|---|
| Slider |  | 5,162 | 2014 | 2026-10-19 | 2014–2026 | 685 | display flag |
| Hackaday Columns |  | 4,892 | 2013 | 2026-10-25 | 2013–2026 | 668 | series |
| Misc Hacks |  | 4,031 | 2005 | 2026-10-16 | 2005–2025 | 44 | subject |
| News |  | 3,724 | 2004 | 2026-10-15 | 2008–2026 | 265 | format |
| Featured |  | 3,163 | 2008 | 2026-10-22 | 2012–2026 | 351 | display flag |
| Tool Hacks |  | 3,129 | 2005 | 2026-10-23 | 2008–2026 | 225 | subject |
| Arduino Hacks |  | 3,095 | 2007 | 2026-10-20 | 2008–2026 | 52 | platform |
| 3d Printer hacks |  | 2,887 | 2012 | 2026-10-24 | 2012–2026 | 325 | subject |
| classic hacks |  | 2,866 | 2004 | 2026-09-29 | 2008–2026 | 198 | subject |
| Robots Hacks |  | 2,506 | 2004 | 2026-10-12 | 2005–2026 | 142 | subject |
| Microcontrollers |  | 2,402 | 2005 | 2026-10-16 | 2010–2026 | 228 | subject |
| Raspberry Pi |  | 1,957 | 2012 | 2026-10-06 | 2012–2026 | 104 | platform |
| LED Hacks |  | 1,952 | 2004 | 2026-10-10 | 2008–2026 | 112 | subject |
| home hacks |  | 1,943 | 2005 | 2026-10-16 | 2008–2026 | 135 | subject |
| hardware |  | 1,835 | 2008 | 2026-10-25 | 2009–2026 | 191 | subject |
| Transportation Hacks |  | 1,710 | 2004 | 2026-10-23 | 2005–2026 | 179 | subject |
| Retrocomputing |  | 1,700 | 2005 | 2026-10-24 | 2018–2026 | 513 | subject |
| Security Hacks |  | 1,602 | 2005 | 2026-10-23 | 2008–2026 | 157 | subject |
| Peripherals Hacks |  | 1,586 | 2004 | 2026-10-23 | 2005–2026 | 163 | subject |
| Musical Hacks |  | 1,561 | 2005 | 2026-10-24 | 2010–2026 | 173 | subject |
| The Hackaday Prize |  | 1,542 | 2014 | 2025-09-25 | 2014–2023 | 1 | contest or event |
| Radio Hacks |  | 1,520 | 2010 | 2026-10-25 | 2010–2026 | 165 | subject |
| computer hacks |  | 1,492 | 2004 | 2026-10-12 | 2004–2026 | 172 | subject |
| Science |  | 1,436 | 2016 | 2026-10-23 | 2018–2026 | 396 | subject |
| cons |  | 1,422 | 2006 | 2026-10-17 | 2007–2026 | 89 | contest or event |
| Original Art | Curated | 1,271 | 2014 | 2026-10-22 | 2015–2026 | 160 | original-content genre (Curated) |
| Software Hacks |  | 1,142 | 2004 | 2026-10-22 | 2011–2026 | 262 | subject |
| Wireless Hacks |  | 1,119 | 2004 | 2026-10-17 | 2005–2026 | 63 | subject |
| Toy Hacks |  | 1,067 | 2004 | 2026-10-07 | 2009–2026 | 96 | subject |
| Interest | Curated | 1,044 | 2015 | 2026-10-14 | 2015–2026 | 147 | original-content genre (Curated) |
| digital cameras hacks |  | 1,040 | 2004 | 2026-10-21 | 2005–2026 | 98 | subject |
| clock hacks |  | 1,031 | 2010 | 2026-10-25 | 2010–2026 | 117 | subject |
| cnc hacks |  | 945 | 2005 | 2026-09-24 | 2008–2025 | 33 | subject |
| how-to |  | 910 | 2005 | 2026-10-22 | 2008–2026 | 64 | format |
| Games |  | 890 | 2004 | 2026-10-23 | 2018–2026 | 245 | subject |
| Software Development |  | 877 | 2005 | 2026-10-20 | 2010–2026 | 176 | subject |
| green hacks |  | 876 | 2009 | 2026-10-20 | 2009–2026 | 105 | subject |
| Wearable Hacks |  | 873 | 2004 | 2026-10-25 | 2008–2026 | 60 | subject |
| Parts |  | 842 | 2008 | 2026-10-21 | 2008–2026 | 154 | format |
| home entertainment hacks |  | 826 | 2004 | 2026-10-25 | 2005–2026 | 54 | subject |
| Hackaday links |  | 820 | 2009 | 2026-10-25 | 2010–2026 | 87 | series |
| digital audio hacks |  | 777 | 2004 | 2026-10-21 | 2008–2026 | 57 | subject |
| Space |  | 740 | 2017 | 2026-10-22 | 2018–2026 | 143 | subject |
| Medical Hacks |  | 723 | 2008 | 2026-10-21 | 2010–2026 | 65 | subject |
| Repair Hacks |  | 710 | 2009 | 2026-10-23 | 2010–2026 | 124 | subject |
| Laser Hacks |  | 688 | 2005 | 2026-10-05 | 2007–2026 | 58 | subject |
| contests |  | 684 | 2006 | 2026-10-24 | 2008–2026 | 94 | contest or event |
| chemistry hacks |  | 667 | 2010 | 2026-10-18 | 2010–2026 | 63 | subject |
| Video Hacks |  | 661 | 2004 | 2026-10-24 | 2009–2026 | 44 | subject |
| Linux Hacks |  | 644 | 2005 | 2026-10-22 | 2009–2026 | 115 | subject |
| Nintendo Hacks |  | 644 | 2004 | 2026-10-01 | 2005–2026 | 45 | subject |
| History | Curated | 640 | 2004 | 2026-10-21 | 2016–2026 | 161 | original-content genre (Curated) |
| Current Events | Curated | 632 | 2015 | 2026-10-22 | 2016–2026 | 92 | original-content genre (Curated) |
| Engineering | Curated | 614 | 2014 | 2026-10-19 | 2016–2026 | 63 | original-content genre (Curated) |
| Teardown |  | 596 | 2009 | 2026-10-22 | 2015–2026 | 124 | format |
| Rants |  | 554 | 2009 | 2026-10-21 | 2015–2026 | 94 | format |
| Cellphone Hacks |  | 546 | 2004 | 2026-10-08 | 2005–2019 | 21 | subject |
| drone hacks |  | 544 | 2012 | 2026-10-21 | 2013–2026 | 43 | subject |
| Podcasts |  | 517 | 2006 | 2026-10-23 | 2019–2026 | 160 | series |
| Art |  | 497 | 2005 | 2026-10-21 | 2018–2026 | 86 | subject |
| car hacks |  | 493 | 2014 | 2026-10-22 | 2015–2026 | 44 | subject |
| handhelds hacks |  | 448 | 2004 | 2026-10-07 | 2005–2026 | 59 | subject |
| Android Hacks |  | 443 | 2008 | 2026-10-25 | 2008–2026 | 31 | subject |
| FPGA |  | 397 | 2013 | 2026-10-13 | 2014–2026 | 31 | platform |
| Holiday Hacks |  | 368 | 2011 | 2025-12-26 | 2011–2023 | 9 | subject |
| Skills | Curated | 367 | 2015 | 2026-10-07 | 2015–2024 | 22 | original-content genre (Curated) |
| Uncategorized |  | 356 | 2004 | 2026-06-21 | 2005–2008 | 1 | format |
| Artificial Intelligence |  | 355 | 2022 | 2026-10-24 | 2022–2026 | 168 | subject |
| Reverse Engineering |  | 355 | 2022 | 2026-10-22 | 2022–2026 | 138 | subject |
| ARM |  | 353 | 2012 | 2026-09-19 | 2012–2022 | 9 | platform |
| cooking hacks |  | 353 | 2005 | 2026-10-10 | 2011–2026 | 29 | subject |
| Lifehacks |  | 320 | 2005 | 2026-09-17 | 2012–2026 | 19 | subject |
| Retrotechtacular | Hackaday Columns | 304 | 2013 | 2026-08-10 | 2013–2025 | 19 | series |
| Network Hacks |  | 286 | 2004 | 2026-10-25 | 2013–2026 | 57 | subject |
| ATtiny Hacks |  | 283 | 2011 | 2026-03-16 | 2011–2022 | 9 | platform |
| Reviews |  | 283 | 2005 | 2026-10-19 | 2015–2026 | 37 | format |
| Weapons Hacks |  | 281 | 2005 | 2026-10-23 | 2011–2024 | 11 | subject |
| Hackerspaces |  | 275 | 2010 | 2026-03-03 | 2011–2018 | 2 | contest or event |
| Mac Hacks |  | 271 | 2004 | 2026-10-19 | 2005–2026 | 35 | subject |
| Crowd Funding |  | 270 | 2012 | 2026-10-06 | 2012–2019 | 3 | contest or event |
| Virtual Reality |  | 260 | 2012 | 2026-10-10 | 2013–2024 | 19 | subject |
| iphone hacks |  | 251 | 2007 | 2026-10-15 | 2008–2016 | 7 | tag in disguise (one product) |
| Nintendo Game Boy Hacks |  | 249 | 2004 | 2026-10-20 | 2005–2022 | 16 | tag in disguise (one product) |
| laptops hacks |  | 236 | 2005 | 2026-05-18 | 2007–2024 | 8 | subject |
| Tech Hacks |  | 234 | 2013 | 2026-10-18 | 2019–2026 | 40 | subject |
| internet hacks |  | 233 | 2013 | 2026-10-22 | 2013–2026 | 51 | subject |
| Phone Hacks |  | 228 | 2004 | 2026-10-22 | 2017–2026 | 36 | subject |
| Solar Hacks |  | 227 | 2005 | 2026-09-24 | 2011–2024 | 22 | subject |
| gps hacks |  | 212 | 2004 | 2026-09-27 | 2005–2012 | 13 | subject |
| Xbox Hacks |  | 206 | 2004 | 2026-10-04 | 2006–2011 | 12 | tag in disguise (one product) |
| Playstation Hacks |  | 205 | 2004 | 2026-10-13 | 2006–2011 | 14 | tag in disguise (one product) |
| Ask Hackaday | Hackaday Columns | 189 | 2010 | 2026-05-14 | 2012–2019 | 10 | series |
| Machine Learning |  | 184 | 2019 | 2026-09-24 | 2019–2025 | 25 | subject |
| Engine Hacks |  | 177 | 2011 | 2026-09-30 | 2011–2022 | 14 | subject |
| ipod hacks |  | 173 | 2004 | 2026-09-27 | 2004–2011 | 4 | tag in disguise (one product) |
| PCB Hacks |  | 143 | 2023 | 2026-10-03 | 2023–2026 | 71 | subject |
| Battery Hacks |  | 136 | 2005 | 2026-09-30 | 2022–2026 | 55 | subject |
| Fail of the Week | Hackaday Columns | 134 | 2013 | 2024-12-04 | 2013–2018 | 0 | series |
| Beer Hacks |  | 129 | 2009 | 2026-01-24 | 2011–2013 | 2 | tag in disguise (one product) |
| Kinect hacks |  | 121 | 2010 | 2025-10-02 | 2011–2012 | 2 | tag in disguise (one product) |
| Cyberdecks |  | 120 | 2018 | 2026-10-24 | 2022–2026 | 25 | subject |
| Nintendo Wii Hacks |  | 115 | 2007 | 2026-08-23 | 2007–2009 | 9 | tag in disguise (one product) |
| Multitouch Hacks |  | 110 | 2008 | 2026-06-19 | 2008–2011 | 4 | tag in disguise (one product) |
| HackIt |  | 106 | 2007 | 2020-02-11 | 2008–2011 | 0 | series |
| Portable Audio Hacks |  | 105 | 2004 | 2026-09-05 | 2005–2007 | 5 | subject |
| downloads hacks |  | 104 | 2004 | 2021-06-05 | 2008–2009 | 0 | format |
| Biography |  | 98 | 2015 | 2025-08-12 | 2018–2020 | 1 | format |
| Tablet Hacks |  | 94 | 2004 | 2026-09-18 | 2012–2012 | 4 | tag in disguise (one product) |
| Interviews |  | 92 | 2010 | 2024-12-08 | 2014–2015 | 0 | format |
| Roundup |  | 88 | 2004 | 2026-08-17 | none | 2 | series |
| google hacks |  | 87 | 2005 | 2026-10-05 | 2008–2009 | 6 | tag in disguise (one product) |
| Curated |  | 76 | 2016 | 2023-03-13 | 2016–2017 | 0 | display flag |
| Business | Curated | 73 | 2015 | 2026-09-26 | 2016–2017 | 5 | original-content genre (Curated) |
| Portable Video Hacks |  | 72 | 2004 | 2026-05-18 | 2008–2009 | 1 | subject |
| High Voltage |  | 72 | 2005 | 2026-09-02 | 2022–2025 | 32 | subject |
| Nintendo DS Hacks |  | 58 | 2004 | 2026-10-25 | 2008–2008 | 8 | tag in disguise (one product) |
| Netbook Hacks |  | 56 | 2008 | 2022-09-25 | 2008–2009 | 0 | tag in disguise (one product) |
| lockpicking hacks |  | 56 | 2004 | 2026-05-05 | none | 4 | subject |
| PSP Hacks |  | 52 | 2005 | 2025-08-17 | none | 2 | tag in disguise (one product) |
| Kindle hacks |  | 38 | 2011 | 2026-07-03 | none | 3 | tag in disguise (one product) |
| g1 hacks |  | 26 | 2008 | 2009-08-29 | 2008–2008 | 0 | tag in disguise (one product) |
| firefox hacks |  | 24 | 2008 | 2015-02-16 | 2008–2008 | 0 | tag in disguise (one product) |
| Weekly Roundup |  | 19 | 2011 | 2016-11-05 | 2011–2011 | 0 | series |
| Fiction | Curated | 16 | 2016 | 2026-03-17 | none | 2 | original-content genre (Curated) |
| blackberry hacks |  | 15 | 2006 | 2025-09-24 | none | 3 | tag in disguise (one product) |
| Hackaday Store |  | 12 | 2014 | 2017-11-23 | none | 0 | contest or event |
| Major Tom |  | 8 | 2014 | 2022-01-25 | none | 0 | series |
| Palm Pre Hacks |  | 6 | 2009 | 2010-07-28 | none | 0 | tag in disguise (one product) |
| Nook Hacks |  | 3 | 2012 | 2026-05-04 | none | 1 | tag in disguise (one product) |

## 4. Series and columns

Hackaday's series are marked by **title**, not reliably by category or tag. The title prefix before a colon ("Hackaday Links: October 25, 2026") is the strongest and most consistent mark; a category exists for only five series (Hackaday links, Retrotechtacular, Ask Hackaday, Fail of the Week, Podcasts), and a series' tag is usually on a fraction of its posts (the tag "hackaday links" is on 354 of 923 Links posts). The category Hackaday Columns covers most series since 2013 but also holds 1,667 unprefixed posts, chiefly the editors' essays.

The category's own description names the series Hackaday thinks of as its columns: "recurring topics and popular series like Linux-Fu, 3D-Printering, Hackaday Links, This Week in Security, Inputs of Interest, Profiles in Science, Retrotechtacular, Ask Hackaday, Teardowns, Reviews, and many more" (https://hackaday.com/category/hackaday-columns/). The About page lists "Our Columns" as Hackaday Links, Making For Others and This Week In Security (https://hackaday.com/about/).

### 4.1 The series, by title

Posts whose title starts with the series name (Appendix A.4 gives the patterns). Median gap is the median number of days between consecutive posts in the series. Writers are the three with the most posts in it.

| Series | Posts | First | Last | Median gap (days) | Posts in 2025 | Usual writers |
|---|--:|---|---|--:|--:|---|
| Hackaday Links | 923 | 2005-02-09 | 2026-10-25 | 7.0 | 48 | Dan Maloney 319, Brian Benchoff 312, Mike Szczys 114 |
| Hackaday Prize Entry | 474 | 2015-04-29 | 2017-11-07 | 1.0 | 0 | Brian Benchoff 263, John Baichtal 41, Jenny List 33 |
| Hackaday Podcast | 396 | 2005-07-08 | 2026-10-23 | 7.0 | 49 | Mike Szczys 135, Tom Nardi 75, Kristina Panos 63 |
| This Week in Security | 368 | 2019-05-01 | 2026-10-23 | 7.0 | 50 | Jonathan Bennett 334, Mike Kershaw 34 |
| Retrotechtacular | 335 | 2012-04-28 | 2026-08-10 | 8.1 | 15 | Kristina Panos 76, Jenny List 61, Al Williams 55 |
| Hack Chat (announcements) | 275 | 2017-01-11 | 2025-03-03 | 7.0 | 3 | Dan Maloney 175, Brian Benchoff 76, Tom Nardi 15 |
| Ask Hackaday | 226 | 2010-03-21 | 2026-06-17 | 15.1 | 12 | Brian Benchoff 37, Al Williams 34, Jenny List 27 |
| Fail of the Week | 160 | 2013-08-08 | 2025-10-20 | 12.5 | 1 | Dan Maloney 46, Mike Szczys 39, Jenny List 17 |
| Linux Fu | 129 | 2017-03-10 | 2026-09-15 | 19.9 | 8 | Al Williams 128 |
| FLOSS Weekly | 122 | 2023-12-21 | 2026-09-16 | 7.0 | 46 | Jonathan Bennett 122 |
| The Hacklet | 115 | 2013-07-27 | 2016-09-24 | 7.0 | 0 | Adam Fabio 105 |
| Teardown | 91 | 2009-08-18 | 2026-10-02 | 27.0 | 6 | Tom Nardi 40, Maya Posch 16, Jenny List 6 |
| Keebin' with Kristina | 88 | 2021-06-21 | 2026-03-19 | 9.0 | 29 | Kristina Panos 87 |
| New Part Day | 82 | 2015-01-16 | 2024-06-16 | 18.2 | 0 | Brian Benchoff 44, Jenny List 13 |
| 3D Printering | 76 | 2013-08-07 | 2026-08-20 | 28.0 | 2 | Brian Benchoff 30, Donald Papp 18, Al Williams 12 |
| Review | 64 | 2009-08-05 | 2026-07-01 | 38.1 | 1 | Jenny List 33, Tom Nardi 8 |
| HackIt | 34 | 2007-11-12 | 2020-02-11 | 22.1 | 0 | Eliot 15, Will O'Brien 9 |
| Tech In Plain Sight | 32 | 2021-08-25 | 2026-09-10 | 36.1 | 5 | Al Williams 29 |
| Developed on Hackaday | 32 | 2013-12-06 | 2016-01-19 | 13.9 | 0 | Mathieu Stephan 28 |
| Books You Should Read | 28 | 2016-06-23 | 2023-09-21 | 48.9 | 0 | Sonya Vasquez 5, several others |
| Hackaday Retro Edition | 25 | 2012-06-12 | 2015-07-12 | 22.2 | 0 | Brian Benchoff 25 |
| Embed With Elliot | 20 | 2015-06-05 | 2017-05-26 | 14.0 | 0 | Elliot Williams 20 |
| Mining and Refining | 17 | 2021-11-15 | 2025-06-24 | 42.0 | 2 | Dan Maloney 17 |
| Circuit VR | 17 | 2018-04-06 | 2022-03-31 | 21.1 | 0 | Al Williams 17 |
| Inputs of Interest | 16 | 2020-01-21 | 2024-02-14 | 35.1 | 0 | Kristina Panos 16 |
| Mechanisms | 15 | 2018-01-26 | 2024-11-07 | 8.0 | 0 | Dan Maloney 12 |
| Automate the Freight | 15 | 2016-11-23 | 2022-06-20 | 26.9 | 0 | Dan Maloney 15 |
| Jenny's Daily Drivers | 14 | 2023-07-10 | 2026-07-02 | 37.0 | 5 | Jenny List 14 |
| How To Get Into Cars | 14 | 2019-12-02 | 2021-12-23 | 44.0 | 0 | Zoe Skyforest 14 |
| Hacking When It Counts | 14 | 2015-09-25 | 2025-07-14 | 90.9 | 1 | Dan Maloney 14 |
| Know Audio | 13 | 2021-06-02 | 2025-12-30 | 33.9 | 3 | Jenny List 13 |
| Hackaday Dictionary | 13 | 2015-09-24 | 2017-01-20 | 21.1 | 0 | Richard Baguley 10 |
| All About USB-C | 13 | 2022-12-06 | 2023-08-07 | 7.0 | 0 | Arya Voronova 13 |
| Spy Tech | 12 | 2017-09-20 | 2026-06-08 | 213.3 | 2 | Al Williams 11 |
| Logic Noise | 12 | 2015-02-04 | 2015-11-05 | 15.9 | 0 | Elliot Williams 12 |

Smaller series active since 2022, by title prefix: Retro Gadgets (10, Al Williams), Big Chemistry (10, Dan Maloney), PCB Design Review (9, Arya Voronova), Hack On Self (8, Arya Voronova), Radio Apocalypse (7, Dan Maloney), Programming Ada (6, Maya Posch), Hacker Tactic (6, Arya Voronova), Share Your Projects (5), Displays We Love Hacking (5), Boss Byproducts (5, Kristina Panos), The $50 Ham (10 in Columns). The front page on 26 October 2026 also showed "The FPGA Chronicles" (Al Williams).

### 4.2 The weekly spine

Since 2019 Hackaday's columns have run on a fixed week (weekdays in GMT, from Appendix A.5):

| Day | Series | Since | Writer now |
|---|---|---|---|
| Sunday (Monday GMT in some weeks) | Hackaday Links | weekly since 2012 (irregular from November 2009); near-daily "hackaday links" in 2005 (110 posts) | Tom Nardi (Dan Maloney 2019 to December 2025) |
| Monday | Hack Chat announcement (the chat itself ran on hackaday.io) | 2017 ("Friday Hack Chat" 2017–2018) to March 2025 | ended |
| Wednesday | FLOSS Weekly (an existing podcast, carried on Hackaday from December 2023, from episode 762) | 2023 | Jonathan Bennett |
| Friday | This Week in Security (360 of 368 on a Friday) | 2019 | Mike Kershaw since March 2026 (Jonathan Bennett 2019–2025) |
| Friday | Hackaday Podcast (345 of the 346 episodes since 2020 on a Friday) | 2019 | Elliot Williams with a rotating co-editor; the post is credited to the co-host |
| Saturday | The editor's essay, unprefixed, filed in Columns (e.g. "Making for Others", 24 October 2026); 209 of the 265 since 2020 are tagged "newsletter", so it appears to be the weekly newsletter's lead | 2020 | Elliot Williams (32 to 45 a year) |

The podcast has two lives: three early episodes in 2005 and 2006 ("hackaday podcast 03"), then the numbered run from "Hackaday Podcast 001" in early 2019 to Episode 392 on 23 October 2026, distributed on iTunes, Spotify, Stitcher, RSS and YouTube (https://hackaday.com/2026/10/23/hackaday-podcast-episode-392-microscopes-drones-and-other-diy-things/). The category Podcasts holds both the Hackaday Podcast and FLOSS Weekly (101 posts in 2024).

### 4.3 Series that were asked about and not found

**"Tech Note"** appears in no title, tag or category in the archive. The nearest series are Tech In Plain Sight (Al Williams, 2021 on) and the old "How-To" and "Beginner concepts" prefixes. "Profiles in Science", named in the Columns description, is a tag (15 posts), with one title using the name.

## 5. Contests, challenges and events

### 5.1 The Hackaday Prize, 2014 to 2023

The Hackaday Prize ran for ten years and ended in 2023: "Hackaday Prize 2023: Ending 10 Years on a High Note" (7 November 2023). The category The Hackaday Prize has 1,542 posts, all from 2014 to 2023 but one teardown in 2025 (https://hackaday.com/category/the-hackaday-prize/). The last two editions ran as "Supplyframe DesignLab: 2022 Hackaday Prize" and "…2023 Hackaday Prize" on hackaday.io (https://hackaday.io/contests).

| Year | Posts in the category | Titles naming the Prize ("Hackaday Prize", "THP") | Year tag (posts) |
|---|--:|--:|---|
| 2014 | 145 | 97 | TheHackadayPrize (25), the hackaday prize |
| 2015 | 255 | 178 | 2015 Hackaday Prize (234) |
| 2016 | 227 | 186 | 2016 Hackaday Prize (234) |
| 2017 | 239 | 213 | 2017 Hackaday Prize (238) |
| 2018 | 242 | 15 | 2018 Hackaday Prize (236) |
| 2019 | 180 | 15 | 2019 Hackaday Prize (209) |
| 2020 | 82 | 11 | 2020 Hackaday Prize (71) |
| 2021 | 73 | 16 | 2021 Hackaday Prize (70) |
| 2022 | 53 | 73 | 2022 Hackaday Prize (72) |
| 2023 | 45 | 66 | 2023 Hackaday Prize (48) |

From 2015 to 2017 the Prize had its own daily series, "Hackaday Prize Entry" (474 posts, a median of one day apart, chiefly Brian Benchoff), plus "Hackaday Prize Semifinalist" (2015), "THP Entry", "THP Semifinalist" and "THP Hacker Bio" (2014) and "Hackaday Prize Worldwide" (2015 meetups, 14 posts). From 2018 the Prize ran as a set of themed challenges (Robotics Module, Power Harvesting, Human Computer Interface, Musical Instrument, 2018).

### 5.2 Challenges and contests

The category **contests** has 684 posts from 2006 to 2026, and runs at 47 to 74 a year since 2022, its busiest years. Hackaday's contests since 2022 have been short themed challenges hosted on hackaday.io, each with its own title prefix and tag. In date order, with the posts that name them:

| Year | Contest (posts by title prefix; by tag) |
|---|---|
| 2013 | Trinket contest (tag, 11); Fubarino Contest (28) |
| 2014 | Sci-Fi Contest (tag "sci-fi contest", 26, 2014–2017); Trinket Everyday Carry Contest (11) |
| 2015 | Caption CERN Contest (tag, 20); The Square Inch Project (three tags of 4, 8 and 5 posts, 2015–2018) |
| 2016 | Enlightened Raspberry Pi Contest (7); 1 kB Challenge (8, into 2017) |
| 2017 | Coin Cell Challenge (two tags, 10 and 8 posts, into 2018) |
| 2018 | Repairs You Can Print (16); the four Prize challenges; Circuit Sculpture (tag, 64, 2018 on) |
| 2021 | 555 Timer Contest (into 2022) |
| 2022 | Sci-Fi Contest (14); Odd Inputs and Peculiar Peripherals (13); Cyberdeck Contest (16); FPV Contest (6) |
| 2023 | Op Amp Challenge (11); Low-Power Challenge; Cyberdeck Challenge (8); Halloween Hackfest (15) |
| 2024 | Home Sweet Home Automation (8); Business Card Challenge (18); Tiny Games Contest (16); Supercon SAO ("Add On") Contest (3 titles, 7 tagged) |
| 2025 | Pet Hacks Contest (12); One Hertz Challenge (45, under two spellings); Component Abuse Challenge (28) |
| 2026 | Green Powered Challenge (10); Frikkin Lasers Challenge (4); Retrocomputing Challenge (10, running on 26 October 2026) |

Names drift between title, tag and hackaday.io: the "2025 One Hertz Challenge" is also "2025 One-Hertz Challenge" in titles and "2025 One Hertz Contest" in tags; the "2024 Tiny Games Contest" is the "Tiny Games Challenge" in tags and on hackaday.io; hackaday.io's "Retrocomputing Contest" is the "2026 Retrocomputing Challenge" in posts.

### 5.3 Events

The category **cons** (1,422 posts, 2006 on) holds both Hackaday's own events and reports from others' (CES, Maker Faire, DEF CON, Chaos Communication Congress: "32C3", "38C3").

| Event | Years | Evidence |
|---|---|---|
| **Hackaday Superconference ("Supercon")** | 2015 to 2019; 2022 to 2026 | first "Hackaday SuperConference Call for Proposal", 4 October 2015; "Welcome Back, Supercon!" 12 November 2022; "Supercon Ten Tickets on Sale Now", 13 August 2026. 28 Supercon tags (one per year and spelling, e.g. "2018 Hackaday Superconference", 79 posts; "Supercon", 76) on 444 posts. Talk write-ups ran as "Supercon 2022:" to "Supercon 2024:" prefixes (64 posts) |
| **Hackaday Remoticon** (online, in place of Supercon) | 2020, 2021 | 58 titles; "Remoticon Video" write-ups (18) |
| **Hackaday Belgrade** | 2016, 2018; 2020 postponed | 27 titles; tags "Hackaday Belgrade" (37), "Hackaday Belgrade 2018" (16) |
| **Hackaday Berlin**, then **Hackaday Europe** | 2023 (Berlin), 2024, 2025, 2026 | "Come Join Us for Hackaday Berlin!" 2023; "Hackaday Europe 2024 is Live"; talk write-ups "Hackaday Europe 2026:" (14) |
| **Hackaday Unconference** | 2017 to 2019 | 13 titles, tag (12) |
| **Meetups** | 2014 to 2022 | tag "meetup" (98 posts); titles "Hackaday Meetup: Shenzhen", "Hackaday x Tindie Meetup in Dublin" |
| **Hack Chat** (weekly online chat on hackaday.io) | 2017 to 2025 | section 4.2 |

## 6. Writers

239 writers are in the archive, of whom 180 wrote posts. Output is concentrated: the 16 writers with over 1,000 posts each wrote 43,733 posts, 74.5% of the archive.

| Posts per writer | Writers | Posts |
|---|--:|--:|
| 1 | 6 | 6 |
| 2 to 10 | 37 | 209 |
| 11 to 50 | 50 | 1,393 |
| 51 to 200 | 49 | 5,036 |
| 201 to 1,000 | 22 | 8,339 |
| over 1,000 | 16 | 43,733 |

The most prolific, with the span of their posts, the number of calendar years in which they posted, their posts in the 12 months to 26 October 2026, and their average length:

| Writer | Posts | First | Last | Years | Last 12 months | Avg words |
|---|--:|---|---|--:|--:|--:|
| Mike Szczys | 6,403 | 2009-08-25 | 2021-12-10 | 13 | 0 | 252 |
| Brian Benchoff | 5,252 | 2011-03-17 (one post dated 2006) | 2019-07-18 | 10 | 0 | 322 |
| Al Williams | 4,592 | 2015-07-19 | 2026-10-23 | 12 | 322 | 433 |
| Zoe Skyforest | 3,808 | 2016-12-08 | 2026-10-25 | 11 | 474 | 368 |
| Jenny List | 3,561 | 2016-02-18 | 2026-10-25 | 11 | 343 | 405 |
| Dan Maloney | 3,373 | 2015-07-01 | 2025-12-22 | 11 | 11 | 451 |
| Tom Nardi | 2,603 | 2017-10-10 | 2026-10-25 | 10 | 119 | 549 |
| Maya Posch | 2,135 | 2018-10-02 | 2026-10-25 | 9 | 746 | 498 |
| Bryan Cockfield | 1,897 | 2014-08-01 | 2026-10-25 | 13 | 198 | 307 |
| Kristina Panos | 1,871 | 2013-12-15 | 2026-10-09 | 14 | 46 | 392 |
| Caleb Kraft | 1,567 | 2008-07-09 | 2013-07-01 | 6 | 0 | 144 |
| Elliot Williams | 1,503 | 2014-09-29 | 2026-10-24 | 13 | 72 | 516 |
| James Hobson | 1,404 | 2013-09-11 | 2018-05-14 | 6 | 0 | 227 |
| Donald Papp | 1,393 | 2016-04-20 | 2026-10-21 | 11 | 172 | 337 |
| Eliot (Phillips) | 1,332 | 2005-05-07 | 2009-09-04 | 5 | 0 | 142 |
| Mike Nathan | 1,039 | 2011-02-04 | 2012-10-11 | 2 | 0 | 215 |
| Will O'Brien | 805 | 2006-08-15 | 2008-07-31 | 3 | 0 | 98 |
| Adam Fabio | 743 | 2013-09-14 | 2026-09-07 | 14 | 2 | 434 |
| Tyler August | 624 | 2025-04-12 | 2026-10-25 | 2 | 388 | 384 |
| Jonathan Bennett | 562 | 2018-09-13 | 2026-09-16 | 9 | 37 | 898 |
| Arya Voronova | 517 | 2021-12-13 | 2026-10-07 | 6 | 21 | 771 |

The writers fall into **generations**, each with its editors: the first writers (Phillip Torrone 2004–2005, then Jason Striegel, Vince Veneziani, Eliot Phillips and Will O'Brien to 2009, short posts of about 100 to 150 words); the Caleb Kraft and Mike Szczys years (2008 to 2013, with Mike Nathan and Jakob Griffith); the Brian Benchoff and Mike Szczys years (2011 to 2019, with James Hobson, the volume growing to 3,500 posts a year); and the present staff (Elliot Williams, Tom Nardi, Al Williams, Jenny List, Dan Maloney, Kristina Panos, Maya Posch, Bryan Cockfield, Donald Papp). A further cohort started in April 2025: Tyler August, Aaron Beckendorf, John Elliot V, Matt Varian and Ian Bos. Posts have grown from about 150 words before 2010 to 417 on average in 2025.

**Staff against contributors.** The About page lists editorial staff and contributors (https://hackaday.com/about/): Elliot Williams (Editor in Chief), Tom Nardi (Managing Editor), Kristina Panos (Assignments Editor), Jenny List (Contributing Editor and European Correspondent), Dan Maloney (Staff Writer and Community Engineer), Adam Fabio (Community Editor), Al Williams and Lewin Day (Staff Writers), Joe Kim (Art Director); 31 "Contributors" (among them Bryan Cockfield, Donald Papp, Maya Posch, Jonathan Bennett); and four "Contributors-at-Large" (Quinn Dunki, Inderpreet Singh, Aleksandar Bradic, Voja Antonic). The data agrees in shape: 28 writers posted in the last 12 months, and seven of them (Maya Posch, Zoe Skyforest, Tyler August, Jenny List, Al Williams, Bryan Cockfield, Donald Papp) wrote 2,643 of those 3,335 posts. The 93 writers with 50 posts or fewer are the occasional contributors; together they wrote 1,608 posts, 2.7% of the archive.

## 7. Tags

### 7.1 How the site uses them

Tags are free text, typed by each writer per post. Tagging was barely used before 2006 (0.9 tags a post that year) and became routine in 2008 (6.2); since then posts carry 4 to 6 tags, falling to 3.9 in 2026. 1,756 posts have no tag, 1,401 of them from 2004 to 2008. On average 4.8 tags a post, 280,212 tag links in all. New tags are created at 2,000 to 4,000 a year (2,052 in 2025), so the set grows without being curated. Each tag has its own archive page (`/tag/<slug>/`), reached only from a post's "Tagged" line; the site has no tag index or cloud.

### 7.2 The distribution

| Posts per tag | Tags | Share of tags | Tag links |
|---|--:|--:|--:|
| 0 | 919 | 1.6% | 0 |
| 1 | 31,900 | 55.9% | 31,900 |
| 2 | 8,288 | 14.5% | 16,576 |
| 3 to 5 | 7,940 | 13.9% | 29,360 |
| 6 to 10 | 3,683 | 6.5% | 27,719 |
| 11 to 50 | 3,566 | 6.3% | 74,070 |
| 51 to 100 | 402 | 0.7% | 28,081 |
| 101 to 500 | 304 | 0.5% | 54,932 |
| over 500 | 20 | 0.04% | 17,574 |

**31,900 tags (56%) are used once**, and 20,793 posts carry at least one such tag. 919 tags have no post at all, with Hackaday's own count also zero. Only 4,292 tags (7.5%) have more than 10 posts, but they carry 62% of the tag links. The largest: arduino (2,951), raspberry pi (1,748), led (1,380), robot (1,037), 3d printing (896), ESP32 (824), linux (713), 3d printed (691), usb (670), cnc (662), clock (658), ESP8266 (652), pcb (647), 3d printer (621), camera (619), bluetooth (608), radio (584), fpga (571), laser (540), security (502).

### 7.3 Duplicates

**By case.** Pure case twins are few: 25 names have two terms each, the second almost always empty and with a "-2" slug ("unity" [unity] 25 posts and "unity" [unity-2] 0; "glass" 70 and "Glass" [glass-working] 0).

**By spacing and punctuation.** Ignoring case, spaces, hyphens, underscores, dots, apostrophes, slashes and brackets, 2,349 groups of 4,709 tags collapse together; ignoring a trailing plural "s" as well joins 1,732 more singular and plural groups. The largest:

| Variants (posts) | Together |
|---|--:|
| 3d printing (896), 3dprinting (13) | 909 |
| ESP32 (824), ESP-32 (23) | 847 |
| ESP8266 (652), ESP-8266 (21) | 673 |
| wifi (475), wi-fi (40) | 515 |
| the hackaday prize (439), TheHackadayPrize (25) | 464 |
| retrocomputing (398), retro computing (31) | 429 |
| home automation (319), HomeAutomation (3) | 322 |
| game boy (175), gameboy (74) | 249 |
| Hack Chat (238), hackchat (9) | 247 |
| woodworking (204), wood working (18) | 222 |
| attiny85 (187), ATTiny 85 (15) | 202 |
| ESP32-S3 (58), esp32s3 (3), esp-32-s3 (1) | 62 |
| 3.5mm (26), 35 mm (2), 3.5-mm (2) | 30 |
| dc-dc (23), DC/DC (4), DC_DC (1) | 28 |

**By family and synonym.** Product families and synonyms split each subject across dozens of tags, many used once. Counting the tags whose names match each family (Appendix A.8):

| Family | Tags | Used once | Tag links | Posts | Largest members |
|---|--:|--:|--:|--:|---|
| Arduino | 89 | 41 | 3,761 | 3,428 | arduino 2,951, arduino nano 175, Arduino Uno 124, arduino mega 96 |
| 3D printing | 195 | 82 | 3,193 | 2,845 | 3d printing 896, 3d printed 691, 3d printer 621, 3d print 131, 3D printed enclosure 111 |
| Raspberry Pi | 93 | 36 | 2,962 | 2,437 | raspberry pi 1,748, Raspberry Pi Pico 139, RPi 129, Raspberry Pi Zero 118, Raspi 101, pi zero 88 |
| Espressif ESP | 51 | 20 | 1,780 | 1,628 | ESP32 824, ESP8266 652, ESP32-S3 58, esp32-C3 29, ESP-32 23, ESP-8266 21, ESP-01 20, ESP32-CAM 18 |
| Hackaday Prize | 17 | 1 | 2,356 | 1,547 | Hackaday Prize 462, the hackaday prize 439, a tag per year 2015–2023 |
| AI and machine learning | 10 | 0 | 1,101 | 773 | ai 363, machine learning 255, neural network 118, artificial intelligence 116, LLM 104, ChatGPT 66 |
| Drones | 49 | 28 | 936 | 647 | drone 399, quadcopter 251, uav 98, multirotor 49, drones 23, quadrotor 20 |
| Supercon | 28 | – | – | 444 | a tag per year and spelling: "2018 Hackaday Superconference", "Supercon", "Hackaday SuperConference" |
| Game Boy | 30 | 11 | 409 | 290 | game boy 175, gameboy 74, game boy advance 48 |

The **ESP32 family** shows the pattern at its plainest: 51 tags, among them ESP32, ESP-32, ESP32-S3, esp32s3, esp-32-s3, esp32-C3, esp32c3, ESP32 C3-mini, ESP32-CAM, ESP32 camera, ESP32-WROOM-32, ESP-WROOM-32, ESP32 wroom, ESP32-WROVER, ESP32-WROVER-32, ESP8266, ESP-8266, ESP8266-01, ESP-01, ESP-12, ESP-12E, esp-12f, esp8285, the misspellings "esp3266" (2) and "ESPP8266" (1), and the SDK "esp-idf". Twenty of the 51 are used once.

Series tags duplicate in the same way: "Hack Chat" (238), "The Hack Chat" (48), "hackchat" (9), "Friday Hack Chat" (5); "hacklet" (98) and "The Hacklet" (69); "This Week in Security" (317) and "This Week in Computer Security" (4).

## 8. Volume

### 8.1 Posts per year

| Year | Posts | Avg words | Writers posting |
|---|--:|--:|--:|
| 2004 (from 5 Sep) | 118 | 67 | 2 |
| 2005 | 598 | 172 | 7 |
| 2006 | 442 | 113 | 5 |
| 2007 | 430 | 108 | 4 |
| 2008 | 1,448 | 165 | 22 |
| 2009 | 1,615 | 157 | 20 |
| 2010 | 1,863 | 182 | 15 |
| 2011 | 2,888 | 215 | 20 |
| 2012 | 3,027 | 213 | 14 |
| 2013 | 2,514 | 242 | 18 |
| 2014 | 2,995 | 301 | 35 |
| 2015 | 3,124 | 404 | 36 |
| 2016 | 3,499 | 476 | 38 |
| 2017 | 3,538 | 441 | 44 |
| 2018 | 3,542 | 464 | 46 |
| 2019 | 3,550 | 450 | 42 |
| 2020 | 3,567 | 449 | 42 |
| 2021 | 3,550 | 448 | 44 |
| 2022 | 3,472 | 432 | 36 |
| 2023 | 3,462 | 423 | 37 |
| 2024 | 3,382 | 422 | 32 |
| 2025 | 3,375 | 417 | 29 |
| 2026 (to 25 Oct) | 2,717 | 415 | 24 |

Hackaday's output grew in two steps (to about 1,500 a year in 2008 and to about 3,000 in 2011) and has been almost constant at 3,400 to 3,570 a year since 2016. There has been at least one post on every day since 1 January 2014.

### 8.2 Posts per month

| Year | Jan | Feb | Mar | Apr | May | Jun | Jul | Aug | Sep | Oct | Nov | Dec |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 2004 | | | | | | | | | 26 | 31 | 30 | 31 |
| 2005 | 31 | 41 | 34 | 38 | 57 | 64 | 71 | 61 | 69 | 57 | 36 | 39 |
| 2006 | 39 | 42 | 40 | 42 | 32 | 31 | 32 | 33 | 35 | 40 | 41 | 35 |
| 2007 | 39 | 32 | 38 | 33 | 33 | 32 | 35 | 43 | 31 | 41 | 40 | 33 |
| 2008 | 36 | 37 | 37 | 43 | 104 | 249 | 210 | 161 | 117 | 133 | 162 | 159 |
| 2009 | 123 | 98 | 92 | 98 | 90 | 102 | 122 | 155 | 221 | 213 | 131 | 170 |
| 2010 | 189 | 138 | 144 | 148 | 152 | 155 | 154 | 152 | 152 | 171 | 170 | 138 |
| 2011 | 178 | 270 | 304 | 247 | 201 | 249 | 240 | 263 | 238 | 260 | 231 | 207 |
| 2012 | 199 | 192 | 212 | 256 | 336 | 295 | 308 | 282 | 266 | 263 | 212 | 206 |
| 2013 | 227 | 207 | 230 | 124 | 131 | 177 | 218 | 212 | 228 | 254 | 241 | 265 |
| 2014 | 248 | 225 | 258 | 249 | 261 | 246 | 262 | 256 | 246 | 250 | 243 | 251 |
| 2015 | 248 | 226 | 251 | 242 | 254 | 247 | 266 | 274 | 268 | 279 | 272 | 297 |
| 2016 | 279 | 262 | 283 | 288 | 297 | 288 | 308 | 296 | 294 | 302 | 296 | 306 |
| 2017 | 299 | 276 | 307 | 294 | 301 | 290 | 296 | 306 | 289 | 301 | 289 | 290 |
| 2018 | 304 | 274 | 301 | 292 | 302 | 290 | 303 | 307 | 288 | 312 | 284 | 285 |
| 2019 | 301 | 278 | 303 | 292 | 304 | 290 | 301 | 302 | 288 | 305 | 290 | 296 |
| 2020 | 305 | 282 | 303 | 293 | 299 | 294 | 306 | 300 | 295 | 305 | 286 | 299 |
| 2021 | 296 | 270 | 305 | 299 | 296 | 291 | 303 | 305 | 291 | 309 | 290 | 295 |
| 2022 | 295 | 268 | 299 | 291 | 294 | 289 | 290 | 295 | 287 | 294 | 276 | 294 |
| 2023 | 293 | 265 | 300 | 284 | 297 | 289 | 295 | 297 | 285 | 296 | 272 | 289 |
| 2024 | 291 | 274 | 295 | 277 | 286 | 270 | 290 | 277 | 278 | 287 | 276 | 281 |
| 2025 | 275 | 263 | 287 | 272 | 291 | 283 | 286 | 289 | 279 | 290 | 271 | 289 |
| 2026 | 287 | 258 | 287 | 277 | 280 | 270 | 282 | 285 | 270 | 221 (to 25th) | | |

266 months; 220.7 posts a month on average; the busiest month is May 2012 (336). Since 2016 every month has had 258 to 312 posts, so a month of Hackaday is a near-constant 280 or so posts and a day about 9 (9.7 on weekdays and 8.0 at weekends in 2025).

### 8.3 The publishing rhythm

Posts go out in scheduled slots: 85.6% of 2025's posts are timed at the top of an hour. Since 2016 the GMT hours 02, 05, 08, 11, 14, 17, 20 and 23 each have about 2,600 posts, the hours between (00, 03, 06, 09, 12, 15, 18, 21) about 1,340, and the others almost none: a post every three hours round the clock, with a second stream on weekdays. The day archive for Sunday 25 October 2026 shows the weekend pattern, seven posts three hours apart from 4 am to 10 pm (https://hackaday.com/2026/10/25/).

## 9. What the structure does not give

- **No section menu.** Hackaday's own menus name no subject. A reader meets subjects only as a post's category links or the flat dropdown.
- **No series index.** No page lists the columns; a series' posts are found by its category (five series), its tag (often on a fraction of its posts), or search.
- **No browsable time archive.** Day archives exist and are linked from each post's date; no month or year archive is linked, and the only pagination is "Older posts".
- **Unreliable tags.** Most tags name one post; families and spellings are unmerged.
- **Category meanings shift.** A category's span and contents depend on refiling (section 1), and four of the largest are display flags or formats, not subjects.

## 10. Judgement: what a viewer would recognise as Hackaday's sections

A regular reader would recognise these as "Hackaday's sections", in roughly this order of recognition. They are the structures to build a teletext hierarchy on; the raw tags and the display flags are not.

1. **The daily stream: the latest hacks.** The front page and the blog are the site. About nine posts a day, slotted every three hours, newest first. In teletext terms this is the news magazine's headlines and story pages, and a day of posts (7 to 10) fits a page of headlines.
2. **The columns, by name.** The recurring series are Hackaday's most recognisable brands and the front page's "Our Columns" block: Hackaday Links (Sunday), This Week in Security (Friday), the Hackaday Podcast (Friday), FLOSS Weekly (Wednesday), the editor's Saturday essay, Retrotechtacular, Ask Hackaday, Linux Fu, Keebin' with Kristina, Tech in Plain Sight, 3D Printering, Fail of the Week and Teardowns; with the finished ones (the Hacklet, New Part Day, Hack Chat, Embed with Elliot, Hackaday Prize Entry) as history. Each is a natural page whose sub-pages are its episodes. They must be identified by title prefix, not by tag or category.
3. **Featured: the long reads.** "If you missed it" and the Curated genres (Original Art, History, Interest, Engineering, Current Events, Skills) are the staff's own writing, distinct from write-ups of readers' hacks, and readers know them as Hackaday's features.
4. **Contests and events.** The Hackaday Prize (2014 to 2023, a page per year), the challenges (several a year since 2022, each with a beginning, entries and winners), Supercon (2015 to 2019, 2022 to 2026), Remoticon, Belgrade and Hackaday Europe. The menu's "Contests" item and the slider's event posts make these visible.
5. **Subjects.** No subject list appears in the menus, but readers know the "X Hacks" categories from every post's "Posted in" line. The 67 subject and 5 platform categories should be merged into a dozen or so subject magazines or pages (for example microcontrollers and boards; 3D printing, tools and CNC; retrocomputing and games; radio; security; robots, drones and transport; science, space and medicine; energy and the home; audio, music and video; computers, software and Linux; art, wearables and toys), with the dead single-product categories folded into their subjects and the display flags and formats left out.
6. **The writers.** Readers know Hackaday's voices (Al Williams, Jenny List, Tom Nardi, Maya Posch, Elliot Williams, Kristina Panos, Dan Maloney, Brian Benchoff, Mike Szczys). The author archive and the About page's staff list support a page per writer for the 30 or so with over 200 posts, and an index of the rest.
7. **Time.** Years and months are the honest axis of the whole archive: a constant 280 posts a month since 2016, and Hackaday's own eras (the first writers 2004–2007, growth 2008–2012, Benchoff and Szczys 2011–2019, the present staff 2016 on) are recognisable in it.

Tags are the one structure not to use as they are: 56% name a single post, and the useful ones exist only as merged families (ESP32, Raspberry Pi, Arduino, 3D printing, drones, AI). They become usable as subject pages only after merging, which is the channel's own tag set's job.

## Appendix A. Key queries

All run as `sqlite3 -readonly ~/hackaday-analysis/analysis.sqlite` on foundintelquiet4060, 26 October 2026.

### A.1 Totals

```sql
select kind, count(*), min(date_gmt), max(date_gmt) from articles group by kind;
select taxonomy, count(*) from terms group by taxonomy;
select count(*) from writers;
```

### A.2 Categories: counts, first and last post, core span, recent posts

```sql
create temp table cy as
  select t.id, substr(a.date_gmt,1,4) y, count(*) n
  from terms t join article_terms at on at.term = t.id
  join articles a on a.id = at.article and a.kind = 'post'
  where t.taxonomy = 'category' group by t.id, y;
select t.name, coalesce((select name from terms p where p.id = t.parent), '') parent,
  count(a.id) posts, substr(min(a.date_gmt),1,4) first, substr(max(a.date_gmt),1,10) last,
  coalesce((select min(y)||'–'||max(y) from cy where cy.id = t.id and n >= 12), 'none') core,
  sum(a.date_gmt >= '2025-01-01') recent
from terms t left join article_terms at on at.term = t.id
left join articles a on a.id = at.article and a.kind = 'post'
where t.taxonomy = 'category' group by t.id order by posts desc;

-- Categories per post
select ncat, count(*) from (select a.id, (select count(*) from article_terms at
  join terms t on t.id = at.term where at.article = a.id and t.taxonomy = 'category') ncat
  from articles a where kind = 'post') group by ncat;

-- Overlap of display flags and columns (repeat for each pair)
create temp table c as select at.article a, t.slug s from article_terms at
  join terms t on t.id = at.term where t.taxonomy = 'category';
select count(*) from c x join c y on x.a = y.a where x.s = 'slider' and y.s = 'featured';
```

### A.3 Title prefixes (finding the series)

```sql
select trim(substr(title,1,instr(title,':')-1)) prefix, count(*) n,
  substr(min(date_gmt),1,10) first, substr(max(date_gmt),1,10) last
from articles where kind = 'post' and instr(title,':') > 0 and instr(title,':') < 40
group by lower(prefix) having n >= 12 order by n desc;
```

### A.4 Series statistics (count, span, median gap, usual writers)

The patterns are `title like` patterns: 'Hackaday Links%', 'This Week in Security%', 'Hackaday Podcast%', 'FLOSS Weekly%', 'Retrotechtacular%', 'Fail of the Week%', 'Ask Hackaday%', 'Linux%Fu%', 'Keebin%', 'New Part Day%', '%Hack Chat%', '3D Printering%', 'Hackaday Prize Entry%', 'Tech In Plain Sight%', 'Teardown%', 'Review:%', '%Hacklet%', 'HackIt%' and the others in section 4.1, each written out in full; '%Tech Note%' matched nothing.

```sql
create temp table pat(name text, like_pat text);
insert into pat values ('Hackaday Links','Hackaday Links%'), ('This Week in Security','This Week in Security%') /* … */;
create temp table s as select p.name sname, a.id, a.date_gmt d, a.writer
  from pat p join articles a on a.kind = 'post' and a.title like p.like_pat;
create temp table g as select sname,
  julianday(d) - julianday(lag(d) over (partition by sname order by d)) gap from s;
create temp table med as select sname, gap from (select sname, gap,
  row_number() over (partition by sname order by gap) rn, count(*) over (partition by sname) c
  from g where gap is not null) where rn = (c+1)/2;
create temp table wr as select sname, w.name, count(*) n,
  row_number() over (partition by sname order by count(*) desc) rn
  from s join writers w on w.id = s.writer group by sname, w.id;
select s.sname, count(*) n, substr(min(d),1,10), substr(max(d),1,10), round(m.gap,1),
  sum(d >= '2025-01-01' and d < '2026-01-01') in2025,
  (select group_concat(name||' '||n, ', ') from wr where wr.sname = s.sname and rn <= 3)
from s left join med m on m.sname = s.sname group by s.sname order by n desc;
```

### A.5 Weekday of a series

```sql
select strftime('%w', date_gmt) dow, count(*) from articles
where title like 'This Week in Security%' group by dow order by count(*) desc;
```

### A.6 Contest and event tags, and events per year

```sql
select t.name, count(a.id), substr(min(a.date_gmt),1,10), substr(max(a.date_gmt),1,10)
from terms t join article_terms at on at.term = t.id join articles a on a.id = at.article
where t.taxonomy = 'post_tag' and (t.name like '%prize%' or t.name like '%challenge%'
  or t.name like '%contest%' or t.name like '%supercon%' or t.name like '%remoticon%'
  or t.name like '%hackaday europe%' or t.name like '%belgrade%' or t.name like '%meetup%')
group by t.id having count(a.id) >= 4 order by min(a.date_gmt);

select substr(date_gmt,1,4) y, sum(title like '%supercon%'), sum(title like '%remoticon%'),
  sum(title like '%hackaday europe%'), sum(title like '%belgrade%'), sum(title like '%meetup%'),
  sum(title like '%unconference%'), sum(title like '%hackaday prize%' or title like 'THP%')
from articles where kind = 'post' group by y;
```

### A.7 Writers

```sql
select w.name, count(a.id) n, substr(min(a.date_gmt),1,10), substr(max(a.date_gmt),1,10),
  count(distinct substr(a.date_gmt,1,4)) years, sum(a.date_gmt >= '2025-10-26') last12m,
  round(avg(a.words)) words
from writers w join articles a on a.writer = w.id and a.kind = 'post'
group by w.id order by n desc;

select case when n = 1 then '1' when n <= 10 then '2-10' when n <= 50 then '11-50'
  when n <= 200 then '51-200' when n <= 1000 then '201-1000' else '>1000' end b,
  count(*), sum(n)
from (select writer, count(*) n from articles where kind = 'post' group by writer)
group by b order by min(n);
```

### A.8 Tags: distribution, duplicates, families

```sql
create temp table tc as select t.id, t.name, t.slug, t.post_count hd, count(at.article) n
  from terms t left join article_terms at on at.term = t.id
  where t.taxonomy = 'post_tag' group by t.id;
select case when n = 0 then '0' when n = 1 then '1' when n = 2 then '2' when n <= 5 then '3-5'
  when n <= 10 then '6-10' when n <= 50 then '11-50' when n <= 100 then '51-100'
  when n <= 500 then '101-500' else '>500' end bucket, count(*), sum(n)
from tc group by bucket order by min(n);

-- Case twins
select group_concat(name||' ['||slug||'] ('||n||')', ' | ') from tc
  group by lower(name) having count(*) > 1;

-- Spacing and punctuation twins
create temp table norm as select id, name, n, lower(replace(replace(replace(replace(replace(
  replace(replace(replace(replace(name,' ',''),'-',''),'_',''),'.',''),'''',''),'’',''),
  '/',''),'(',''),')','')) k from tc;
select count(*), sum(c) from (select k, count(*) c from norm group by k having c > 1);
select count(*) from (select case when k like '%s' then substr(k,1,length(k)-1) else k end k2,
  count(distinct k) c from norm group by k2 having c > 1);

-- Families: each family is a list of lower-case like patterns, for example
-- ESP: 'esp%32%', 'esp%8266%', 'esp-%', 'esp8285%', 'esp3266', 'espp8266', 'esp32%'
create temp table fam(f text, pat text);
insert into fam values ('ESP (Espressif)','esp%32%') /* … */;
create temp table m as select distinct f, tc.id, tc.name, tc.n from fam join tc on lower(tc.name) like fam.pat;
select f, count(*) tags, sum(n) links, sum(n = 1) once,
  (select count(distinct at.article) from article_terms at
     where at.term in (select id from m m2 where m2.f = m.f)) posts
from m group by f order by posts desc;

-- Supercon family, without superconductors
select count(*), (select count(distinct at.article) from article_terms at join terms t2
  on t2.id = at.term where t2.taxonomy = 'post_tag' and lower(t2.name) like '%supercon%'
  and lower(t2.name) not like '%superconduct%')
from terms where taxonomy = 'post_tag' and lower(name) like '%supercon%'
  and lower(name) not like '%superconduct%';

-- Tags per post by year
select substr(a.date_gmt,1,4) y, round(avg((select count(*) from article_terms at
  join terms t on t.id = at.term where at.article = a.id and t.taxonomy = 'post_tag')),1)
from articles a where kind = 'post' group by y;
```

### A.9 Volume and rhythm

```sql
select substr(date_gmt,1,4) y, count(*), round(avg(words)) from articles where kind = 'post' group by y;
select substr(date_gmt,1,4) y, sum(substr(date_gmt,6,2) = '01') Jan, /* … */ sum(substr(date_gmt,6,2) = '12') Dec
  from articles where kind = 'post' group by y;
select substr(date_gmt,12,2) hour, count(*) from articles where kind = 'post' and date_gmt >= '2016' group by hour;
select round(100.0 * sum(substr(date_gmt,15,2) = '00') / count(*), 1)
  from articles where kind = 'post' and date_gmt like '2025%';
-- Days without a post since 2014
with recursive d(x) as (select '2014-01-01' union all select date(x,'+1 day') from d where x < '2026-10-24')
select count(*) from d where x not in (select substr(date_gmt,1,10) from articles where kind = 'post');
```
