# The Hackaday channel's new hierarchy: the design

The design that `hackaday-hierarchy.md` (requirement H.6 item 6, work step 11) asks for: every magazine, page and sub-page of the Hackaday channel rebuilt around the whole archive, with its number, its title and what fills it. Drafted on 26 October 2026 from the analysis in `~/hackaday-analysis/` on foundintelquiet4060. **For the owner's review; nothing is built.** The channel cannot be built to it until the standard's limits are in the engine, the server and the browser (`neoteletext-common/STANDARD-LIMITS.md` section 7) and comments have their own path (requirement H.2).

The owner's frame for it: "THE HACKADAY SITE STRUCTURE IS GOOD BUT NOT A REQUIREMENT - YOU ARE TRYING TO COME UP WITH A BETTER STRUCTURE". Hackaday's own structure (`hackaday-site-structure.md`) is input: its series, contests, events and writers are kept as views; its categories are not the frame.

## 1. What it is built from

| Input | Count | How it was made |
|---|--:|---|
| Posts | 58,726 (2004 to 26 October 2026) | The channel's archive, copied without comments or media (H.3) |
| Readings | 58,724 | Claude Haiku 5.5 through the Batches API, thinking off, the full text of each post: a summary, its kind, topics, themes, which of its Hackaday tags apply and new tags. Two posts were refused by the model and keep only their own data |
| Tags | 49,910 Hackaday tags after merging 57,022 (3,039 on 20 posts or more); 45,265 new, 636 of them on 5 posts or more | Spelling groups, then embedding neighbours, merged by Haiku; a merge that took in a broader tag ("hole" into "through hole") was refused (434 members) |
| Areas and subjects | 36 areas, 268 subjects | Haiku proposed 43 areas from the readings' themes and split each into subjects; the set was then curated by hand: duplicates across areas joined, misplaced subjects moved, overlapping areas merged |
| Subjects of posts | 58,724 main subjects, 84,587 others | Haiku chose each post's main subject from the 268 (an enum, so always one of them) and up to two others |
| Podcast | 397 episodes; 6,232 stories (299 hours) | Every episode fetched; 130 transcribed with Parakeet, 267 kept from Whisper; Haiku sliced each episode into stories, leaving out the opening, the close, sponsors, listener mail and What's That Sound; each story's audio cut as the engine's Opus profile (decision E.123) |

The model's cost for all of it was about $20 at batch prices.

## 2. The rules

1. **A number means a thing, forever.** A subject's page, a series' page, a writer's list and an article's home keep their numbers for the life of the channel. New subjects take free numbers; a subject that is merged leaves a page that says where it went. This fixes section 2 fault 2 of `hackaday-hierarchy.md` (numbers that move daily).
2. **An article lives once and is listed everywhere.** Each article has one *home*: a fixed run of sub-pages holding its text, its picture, its links and where else it appears. Every other place it belongs (its subjects, its series, its writer, its date, its tags) is a *list* whose rows link to the home. An article appears in as many places as it belongs (H.1), at the cost of a row, not of a copy.
3. **Lists grow at their end.** A list's sub-pages are in date order, oldest first, 18 posts a sub-page, so a sub-page once filled keeps its posts and its number; the browser opens a list at its newest sub-page (the engine's rule for rotating pages, REQ-NAV-005).
4. **Keyable pages are for browsing, hexadecimal pages for storage.** The 800 decimal pages hold everything a viewer looks for: the magazines' fronts, the areas, the subjects, the series, the years. The 1,222 hexadecimal pages (`STANDARD-LIMITS.md` section 1), reached only by links, hold the homes and the long lists (writers and tags).
5. **Comments take no number** (H.2): they are attached to an article's home by the browser, never a page or sub-page.
6. **Every limit is the standard's:** 8 magazines, 2,022 pages, 8,190 sub-pages a page, numbered by `neoteletext_core::limits`'s function from place to sub-code. Nothing here needs more; section 9 counts what it uses.

## 3. Homes: where an article lives

**An article's home** is 32 consecutive places (sub-pages) of a store page in its magazine's hexadecimal pages: 255 articles a page (255 × 32 = 8,160 places of the 8,190 a page may have). The magazine is that of the article's main subject when it is first filed; the article is never moved after, even if its subjects change. Articles are numbered in each magazine in the order they arrive, so a home, once given, never moves.

| Places | What |
|---|---|
| 1 to 28 | The text: "Story 1/n" to "Story n/n", 14 rows of text on the first and 17 on each after (the current channel's layout). 96.9% of posts need 16 text sub-pages or fewer, 99.4% need 24; the 0.6% that need more end at the 28th with a QR code to the rest |
| 29 | "Picture": the lead picture as a Level 2.5 mosaic (section 3 idea 3) |
| 30 | "Links": QR codes to the full article and its first videos (idea 4), and its other links |
| 31 | "Where it appears": its subjects, series, writer, date and tags, each with its page number, as links |
| 32 | Spare, for what is added later (for example its podcast stories) |

Places that an article does not use are left empty; a page's catalogue lists only the sub-pages that exist (`STANDARD-LIMITS.md` section 4), so an empty place costs nothing on the wire.

**Store pages in each magazine** are its hexadecimal pages from MA0 upward (MA0 to MFC, 93 pages), then MxA to MxF (60). Magazine 4, the largest, needs 48 of its 153 for its 12,033 articles today and about 3 more a year, so it has room for 35 years at today's rate; magazine 1, which also holds the long lists, has 23 free (section 5).

**A podcast story's home** is the same shape: 1 "Listen" (title, summary, the hosts, the episode and the story's time in it, and its audio, as Opus records when audio is carried, issue #223 of the engine), 2 to 29 the transcript, 30 "Articles" (the posts it discusses, as links to their homes), 31 "Where it appears", 32 spare. The 6,232 stories need 25 store pages, in magazine 8 (section 6).

## 4. A magazine

Every magazine has the same template, so a viewer who learns one knows them all (idea 1).

| Page | What |
|---|---|
| N00 | The magazine's front: its newest headlines across its areas, and its areas with their numbers |
| N01 | Its feature: the newest long read (Hackaday's Featured posts) in the magazine |
| N02 | Its latest: every new post in the magazine, a sub-page a day |
| N03 | Its index: areas and subjects A to Z with their page numbers |
| N04 to N09 | Its features and series with a pulse (idea 6), set per magazine |
| N10 to N99 | Its areas: each area on the first page of a decade, its subjects on the pages after it; an area of more than nine subjects runs into the next decade |

**An area's page** lists its subjects with their counts and the area's newest headlines. **A subject's page** has an "About" sub-page first (what it covers, its counts and years, its most used tags as links, related subjects), then its list (rule 3), articles and podcast stories together, stories marked as such.

## 5. Magazine 1: Hackaday

Magazine 1 is Hackaday itself: its front, its series, its podcast, its people, its community and the ways into the whole archive. It fixes fault 5 (a cupboard numbered in no order): each decade is one kind of thing.

| Page | What |
|---|---|
| 100 | The front: the Jolly Wrencher, its two-second emblem, the ticker, the newsflash and its chime (idea 5) |
| 101 | Headlines: all new posts, a sub-page a day (idea 10) |
| 102 | The magazines: the eight, with their areas |
| 103 | Subjects A to Z: all 266, with their page numbers |
| 104 | Tags A to Z: the 3,039 tags on 20 posts or more, with links to their lists |
| 105 | Search of the whole archive (idea 9) |
| 106 | On this day, across every year since 2004 (idea 8) |
| 107 | Hack roulette, with its wheel (idea 8) |
| 108 | My Hackaday: favourites (idea 9) |
| 109 | Help, in four parts (idea 10) |
| 110 | Series: every series with its page or list |
| 111 | Hackaday Links (923 posts, weekly since 2012) |
| 112 | This Week in Security (368, 2019 on) |
| 113 | Retrotechtacular (335) |
| 114 | Ask Hackaday (226) |
| 115 | Fail of the Week (160) |
| 116 | Teardown (91) and Review (64) |
| 117 | Linux Fu (129) |
| 118 | Keebin' with Kristina (88) |
| 119 | The editor's Saturday essay (the newsletter's lead, 2020 on) |
| 120 | The Podcast: the newest episode, its stories as links |
| 121 | Episodes: a sub-page an episode, oldest first, each listing its stories with their times |
| 122 | The hosts, as they introduce themselves (idea 7) |
| 123 | The soundboard (idea 7) |
| 124 | What's That Sound: the game's rounds, from whatsthatsound |
| 125 | FLOSS Weekly (122, 2023 on) |
| 126 to 129 | Free |
| 130 | Writers: all 182, with their counts and years, as links to their lists |
| 131 | The people of Hackaday: portraits, their own names in their voices, their posts (idea 7) |
| 132 | Facts and figures, and behind the scenes (idea 10) |
| 133 | The reader: any post by its number or address (idea 8) |
| 134 | The quiz, "Guess the magazine" (idea 9) |
| 135 | The live poll (idea 9) |
| 136 | Then and now (idea 8) |
| 137 to 139 | Free |
| **140** | **Hacker Community** (area), its subjects at 141 to 146 (section 6) |
| 147 | The contest running now, with its countdown (idea 6) |
| 148 | Supercon, 2015 to 2026, a sub-page a year |
| 149 | Hack Chat, 2017 to March 2025 (ended) |
| 150 to 172 | The years, 2004 to 2026: a page a year, its posts in date order, a sub-page a day; 173 to 179 for the years to come |
| **180** | **Industry and History** (area), its subjects at 181 to 188 |
| 189 to 199 | Free |

**Hexadecimal pages of magazine 1:** its store (15 pages for its 3,793 articles), the writers' lists (13 pages: 15 writers a page, 512 places each, enough for 9,216 posts; the busiest writer has 6,406), the smaller series' lists (one page: 63 series, 128 places each), and the tags' lists (99 pages: 31 tags a page, 256 places each, enough for 4,608 posts; the busiest tag, "arduino", has 3,780). 128 of its 151 hexadecimal pages.

## 6. The magazines and their subjects

Each subject with the posts whose main subject it is ("Main") and all the posts it lists ("All": main and other subjects). Two subjects proposed were too thin for a page and are joined to their neighbours: "IoT Cloud and Remote Access" (47 posts) to Smart Home Platforms, "Festivals and Public Art" (42) to Interactive Installations. Podcast stories are not in these counts: their 6,232 main subjects and 8,619 others add 14,851 rows to the lists.

Magazines 2 to 8 have pages N00 to N09 as section 4 says; their features at N04 to N09 are to be chosen (for example DOOM watch in Computers, idea 6).

### Magazine 1: Hackaday (3,793 articles' main subject)

| Page | Area or subject | Main | All |
|---|---|--:|--:|
| **140** | **Hacker Community** (area) | 3,234 | |
| 141 | Hackerspaces and Makerspaces | 571 | 1,049 |
| 142 | Conferences and Events | 438 | 936 |
| 143 | Hackaday Prize and Contests | 252 | 967 |
| 144 | Right to Repair | 101 | 324 |
| 145 | Hackaday Columns and Roundups | 1,524 | 1,901 |
| 146 | Hacker Culture | 348 | 760 |
| **180** | **Industry and History** (area) | 559 | |
| 181 | Crowdfunding | 58 | 321 |
| 182 | Industry and Manufacturing | 125 | 412 |
| 183 | Chip Supply and Shortages | 35 | 92 |
| 184 | Companies and Business | 38 | 369 |
| 185 | Careers and Engineering Work | 42 | 87 |
| 186 | History of Technology | 40 | 375 |
| 187 | Military and Cold War Technology | 71 | 261 |
| 188 | Weapons and Firearms | 150 | 269 |

### Magazine 2: Making (6,767 articles' main subject)

| Page | Area or subject | Main | All |
|---|---|--:|--:|
| **210** | **3D Printing** (area) | 3,219 | |
| 211 | Printers and Printer Mods | 668 | 1,228 |
| 212 | Resin Printing | 169 | 284 |
| 213 | Filament and Print Materials | 293 | 645 |
| 214 | Slicers and Print Software | 161 | 346 |
| 215 | Printed Parts and Mechanisms | 340 | 1,297 |
| 216 | Printed Projects | 1,166 | 3,287 |
| 217 | Unusual 3D Printing | 422 | 561 |
| **220** | **Machining and Metalworking** (area) | 2,656 | |
| 221 | CNC Mills and Routers | 496 | 1,065 |
| 222 | Lathes and Manual Machining | 262 | 557 |
| 223 | Laser Cutting and Engraving | 431 | 989 |
| 224 | Casting and Moldmaking | 272 | 531 |
| 225 | Welding, Forging and Blacksmithing | 185 | 434 |
| 226 | Metal Finishing and Plating | 136 | 367 |
| 227 | Mechanical Design and Mechanisms | 273 | 1,255 |
| 228 | Workshop and Hand Tools | 601 | 1,413 |
| **230** | **Woodworking and Crafts** (area) | 892 | |
| 231 | Furniture and Woodworking Projects | 319 | 717 |
| 232 | Woodshop Machines | 115 | 246 |
| 233 | Woodturning and Wooden Mechanisms | 50 | 157 |
| 234 | Composites and Adhesives | 64 | 309 |
| 235 | Textiles, Sewing and E-Textiles | 148 | 300 |
| 236 | Paper, Bookbinding and Printing | 76 | 149 |
| 237 | Glass, Ceramics and Jewellery | 45 | 132 |
| 238 | Concrete and Building Materials | 75 | 167 |

### Magazine 3: Electronics (10,592 articles' main subject)

| Page | Area or subject | Main | All |
|---|---|--:|--:|
| **310** | **Circuits and Components** (area) | 1,190 | |
| 311 | Analog Circuits | 314 | 842 |
| 312 | Digital and Discrete Logic | 265 | 808 |
| 313 | Components and Parts | 345 | 1,257 |
| 314 | Valves and Vacuum Tubes | 96 | 333 |
| 315 | Electronics History | 170 | 541 |
| **320** | **Chips, PCBs and Logic Design** (area) | 2,132 | |
| 321 | FPGA and Programmable Logic | 380 | 750 |
| 322 | CPU and Processor Design | 225 | 428 |
| 323 | Memory and Data Storage | 372 | 754 |
| 324 | PCB Design and Layout | 345 | 1,050 |
| 325 | PCB Making and Assembly | 539 | 1,118 |
| 326 | Chip Design and Semiconductors | 165 | 302 |
| 327 | Open Hardware | 106 | 947 |
| **330** | **Power and Energy** (area) | 2,582 | |
| 331 | Power Supplies and Converters | 363 | 1,060 |
| 332 | Batteries and Charging | 549 | 1,438 |
| 333 | High Voltage and Tesla Coils | 215 | 371 |
| 334 | Low Power and Energy Harvesting | 209 | 438 |
| 335 | Magnetics and Electromagnets | 141 | 411 |
| 336 | Cooling and Thermal Design | 298 | 655 |
| 337 | Mains Wiring and Electrical Safety | 163 | 553 |
| 338 | Solar Power | 320 | 665 |
| 339 | Wind, Water and Other Generation | 160 | 252 |
| 340 | Energy Storage and the Grid | 164 | 457 |
| **350** | **Test and Measurement** (area) | 1,094 | |
| 351 | Oscilloscopes | 296 | 555 |
| 352 | Logic Analysers and Debugging | 106 | 179 |
| 353 | Multimeters and Meters | 173 | 366 |
| 354 | Bench Supplies and Electronic Loads | 80 | 148 |
| 355 | RF and Spectrum Analysis | 42 | 115 |
| 356 | Signal Generators and Simulation | 95 | 172 |
| 357 | Vintage Test Equipment | 187 | 395 |
| 358 | Metrology and Precision Measurement | 115 | 236 |
| **360** | **Sensors** (area) | 945 | |
| 361 | Weather and Environmental Sensing | 317 | 533 |
| 362 | Motion and Position Sensing | 228 | 1,026 |
| 363 | Distance and Ranging | 139 | 427 |
| 364 | Force, Load and Touch | 87 | 265 |
| 365 | Light, Gas and Chemical Sensing | 61 | 462 |
| 366 | Data Logging | 113 | 387 |
| **370** | **Displays and Lighting** (area) | 2,649 | |
| 371 | LED Matrices and Addressable LEDs | 572 | 1,562 |
| 372 | Nixie, VFD and Segment Displays | 402 | 712 |
| 373 | POV and Volumetric Displays | 181 | 224 |
| 374 | E-Paper, OLED and LCD | 267 | 669 |
| 375 | CRTs and Video Output | 130 | 353 |
| 376 | Video Capture and Streaming | 216 | 667 |
| 377 | Projectors and Lasers | 239 | 385 |
| 378 | Lighting and Lamps | 642 | 1,172 |

### Magazine 4: Computers (12,033 articles' main subject)

| Page | Area or subject | Main | All |
|---|---|--:|--:|
| **410** | **Microcontrollers and Embedded** (area) | 4,665 | |
| 411 | Arduino and AVR | 903 | 5,461 |
| 412 | ESP32 and ESP8266 | 656 | 2,070 |
| 413 | ARM and Cortex-M | 291 | 1,323 |
| 414 | PIC and Other Microcontrollers | 155 | 858 |
| 415 | Raspberry Pi and Single-Board Computers | 1,116 | 3,132 |
| 416 | Embedded Linux | 231 | 827 |
| 417 | Firmware and Development Tools | 378 | 1,348 |
| 418 | Buses, USB and Interfacing | 297 | 1,137 |
| 419 | Badges and Wearable Electronics | 638 | 1,093 |
| **420** | **Computers and Peripherals** (area) | 2,584 | |
| 421 | Laptops and Netbooks | 288 | 477 |
| 422 | Handhelds and Cyberdecks | 458 | 781 |
| 423 | Desktop Builds and Case Mods | 239 | 647 |
| 424 | Servers and Self-Hosting | 78 | 196 |
| 425 | Keyboards | 633 | 944 |
| 426 | Mice and Pointing Devices | 175 | 466 |
| 427 | Touch, Gesture and Haptics | 224 | 549 |
| 428 | VR and AR | 245 | 381 |
| 429 | Networking and Routers | 142 | 376 |
| 430 | Printers and Scanners | 102 | 306 |
| **440** | **Retrocomputing** (area) | 1,881 | |
| 441 | 8-Bit Home Computers | 203 | 787 |
| 442 | Vintage PCs and DOS | 286 | 913 |
| 443 | Mainframes and Minicomputers | 65 | 189 |
| 444 | Homebrew Computers | 493 | 857 |
| 445 | Restoration and Preservation | 528 | 1,307 |
| 446 | Computing History | 236 | 822 |
| 447 | Demoscene and Retro Software | 70 | 148 |
| **450** | **Programming** (area) | 915 | |
| 451 | Compilers and Language Design | 163 | 409 |
| 452 | Python and Scripting | 150 | 704 |
| 453 | C, C++, Rust and Go | 40 | 174 |
| 454 | Forth, Lisp and Esoteric Languages | 81 | 156 |
| 455 | Algorithms and Computer Science | 62 | 179 |
| 456 | Open Source and Licensing | 117 | 586 |
| 457 | Game Development | 302 | 622 |
| **460** | **Operating Systems and Software** (area) | 1,224 | |
| 461 | Linux and the Command Line | 346 | 725 |
| 462 | Windows, macOS and Other Systems | 123 | 258 |
| 463 | Historic Operating Systems | 73 | 317 |
| 464 | Kernels and System Internals | 116 | 240 |
| 465 | Web and Browsers | 124 | 414 |
| 466 | CAD and Design Software | 88 | 220 |
| 467 | Emulation and Virtualisation | 169 | 351 |
| 468 | Phones and Mobile Software | 185 | 504 |
| **470** | **Artificial Intelligence** (area) | 764 | |
| 471 | Neural Networks and Machine Learning | 167 | 451 |
| 472 | Language Models and Chatbots | 189 | 393 |
| 473 | Generative AI | 66 | 193 |
| 474 | Computer Vision | 222 | 711 |
| 475 | Speech and Voice | 75 | 216 |
| 476 | Edge AI | 31 | 112 |
| 477 | AI and Society | 14 | 66 |

### Magazine 5: Security and Radio (6,157 articles' main subject)

| Page | Area or subject | Main | All |
|---|---|--:|--:|
| **510** | **Computer Security** (area) | 1,985 | |
| 511 | Vulnerabilities and Exploits | 719 | 1,585 |
| 512 | Cryptography | 195 | 648 |
| 513 | Network and Wireless Security | 145 | 641 |
| 514 | Surveillance and Privacy | 294 | 673 |
| 515 | Locks and Physical Security | 154 | 319 |
| 516 | Car and IoT Security | 24 | 109 |
| 517 | Malware and Attacks | 89 | 349 |
| 518 | Security Roundups and Conferences | 365 | 457 |
| **520** | **Reverse Engineering** (area) | 1,778 | |
| 521 | Firmware Dumping and Analysis | 217 | 488 |
| 522 | Protocol Sniffing | 170 | 476 |
| 523 | Teardowns | 405 | 1,045 |
| 524 | Chip Decapping and Die Imaging | 110 | 174 |
| 525 | Failures and Counterfeits | 91 | 216 |
| 526 | Repurposing Old Hardware | 733 | 2,402 |
| 527 | Fault Injection and Glitching | 52 | 102 |
| **530** | **Radio and Telecoms** (area) | 1,972 | |
| 531 | Software-Defined Radio | 338 | 949 |
| 532 | Antennas and RF Design | 298 | 635 |
| 533 | Bluetooth, Wi-Fi and Zigbee | 162 | 862 |
| 534 | LoRa and Long-Range Radio | 73 | 149 |
| 535 | Remote Controls | 218 | 458 |
| 536 | Mobile Phones and Cellular | 204 | 538 |
| 537 | Telephones and Phreaking | 212 | 392 |
| 538 | GPS and Navigation | 133 | 284 |
| 539 | RFID, NFC and Wireless Power | 107 | 270 |
| 540 | Vintage Radio and Broadcasting | 227 | 513 |
| **550** | **Amateur Radio** (area) | 422 | |
| 551 | Morse Code and CW | 84 | 196 |
| 552 | Homebrew Transceivers | 132 | 377 |
| 553 | Handheld and Mobile Radios | 59 | 175 |
| 554 | Digital Modes | 43 | 121 |
| 555 | Balloons and Beacons | 52 | 110 |
| 556 | Ham Community and Licensing | 52 | 173 |

### Magazine 6: Machines and Space (6,799 articles' main subject)

| Page | Area or subject | Main | All |
|---|---|--:|--:|
| **610** | **Robotics** (area) | 2,361 | |
| 611 | Mobile Robots | 842 | 1,364 |
| 612 | Walking Robots | 344 | 477 |
| 613 | Robot Arms | 668 | 998 |
| 614 | Underwater and Marine Robots | 79 | 104 |
| 615 | Soft Robots | 59 | 104 |
| 616 | Combat Robots | 71 | 105 |
| 617 | Animatronics and Robot Toys | 205 | 410 |
| 618 | Robot Software and Control | 93 | 518 |
| **620** | **Motors and Motion** (area) | 503 | |
| 621 | Steppers and Servos | 117 | 953 |
| 622 | Brushless and DC Motors | 109 | 416 |
| 623 | Pneumatics and Hydraulics | 91 | 280 |
| 624 | Linear Motion and Actuators | 47 | 133 |
| 625 | Unusual Motors and Engines | 139 | 235 |
| **630** | **Flight** (area) | 1,070 | |
| 631 | Multirotors and Quadcopters | 465 | 717 |
| 632 | RC Planes and Gliders | 80 | 192 |
| 633 | Flight Controllers and Autopilots | 62 | 302 |
| 634 | Jet Engines and Propulsion | 77 | 112 |
| 635 | Aviation and Aircraft | 156 | 292 |
| 636 | Drone Rules and Airspace | 62 | 102 |
| 637 | Balloons, Airships and Odd Craft | 99 | 135 |
| 638 | Flight Simulators and Avionics | 69 | 102 |
| **640** | **Vehicles** (area) | 1,614 | |
| 641 | Electric Vehicles and Conversions | 454 | 769 |
| 642 | Car Electronics and Hacking | 252 | 544 |
| 643 | Self-Driving Vehicles | 154 | 253 |
| 644 | Bikes and E-Bikes | 169 | 383 |
| 645 | Motorcycles, Karts and Racing | 111 | 227 |
| 646 | Boats and Submarines | 102 | 206 |
| 647 | Trains and Model Railways | 82 | 127 |
| 648 | Vehicle Repair and Restoration | 290 | 620 |
| **650** | **Space** (area) | 1,251 | |
| 651 | Rockets and Launches | 188 | 384 |
| 652 | Human Spaceflight | 91 | 256 |
| 653 | Moon and Mars | 118 | 251 |
| 654 | Satellites and CubeSats | 327 | 642 |
| 655 | Telescopes and Astronomy | 227 | 412 |
| 656 | Planets and Deep Space | 52 | 184 |
| 657 | Space History | 111 | 391 |
| 658 | Amateur Rocketry | 137 | 219 |

### Magazine 7: Science and Home (6,324 articles' main subject)

| Page | Area or subject | Main | All |
|---|---|--:|--:|
| **710** | **Science** (area) | 1,363 | |
| 711 | Home Chemistry | 269 | 580 |
| 712 | Microscopy | 149 | 241 |
| 713 | Physics Experiments | 152 | 374 |
| 714 | Radiation and Nuclear | 346 | 457 |
| 715 | Quantum Technology | 150 | 203 |
| 716 | Materials Science | 141 | 731 |
| 717 | Spectroscopy and Lab Instruments | 78 | 156 |
| 718 | Vacuum and Cryogenics | 45 | 144 |
| 719 | Climate and Earth Science | 33 | 76 |
| **720** | **Health and Biology** (area) | 1,096 | |
| 721 | Prosthetics and Mobility Aids | 124 | 179 |
| 722 | Assistive Technology | 328 | 556 |
| 723 | Brain and Neural Interfaces | 137 | 241 |
| 724 | Health Monitoring and Medical Devices | 309 | 623 |
| 725 | Pandemic and PPE | 70 | 136 |
| 726 | Biotech and Biohacking | 128 | 317 |
| **730** | **Home and Garden** (area) | 1,357 | |
| 731 | Smart Home Platforms | 611 | 1,357 |
| 732 | Voice Assistants and Smart Speakers | 73 | 187 |
| 733 | Doors, Locks and Garages | 48 | 126 |
| 734 | Lights and Blinds | 34 | 157 |
| 735 | Heating, Cooling and Air | 49 | 123 |
| 736 | Garden and Plants | 158 | 264 |
| 737 | Energy Monitoring | 74 | 183 |
| 738 | Appliance Hacks | 57 | 285 |
| 739 | Pets and Animals | 127 | 247 |
| 740 | Food, Cooking and Drink | 126 | 392 |
| **750** | **Clocks and Time** (area) | 1,030 | |
| 751 | Digital Clocks | 320 | 572 |
| 752 | Mechanical Clocks | 230 | 467 |
| 753 | Watches | 102 | 149 |
| 754 | Time Sources and Synchronisation | 74 | 186 |
| 755 | Timers and the One Hertz Challenge | 88 | 149 |
| 756 | Unusual Clocks | 216 | 267 |
| **760** | **Photography and Optics** (area) | 1,104 | |
| 761 | Camera Hacks | 527 | 1,013 |
| 762 | 3D Scanning and Photogrammetry | 104 | 211 |
| 763 | Thermal, Infrared and X-Ray Imaging | 114 | 201 |
| 764 | Optics and Lenses | 135 | 426 |
| 765 | Film and Photographic Techniques | 65 | 245 |
| 766 | Camera Rigs and Motion Control | 159 | 244 |
| **770** | **Learning** (area) | 374 | |
| 771 | Classroom Projects | 158 | 390 |
| 772 | STEM Outreach and Kids | 9 | 73 |
| 773 | Learning Electronics | 118 | 691 |
| 774 | Books, Courses and Tutorials | 89 | 466 |

### Magazine 8: Sound and Play (6,259 articles' main subject)

| Page | Area or subject | Main | All |
|---|---|--:|--:|
| **810** | **Audio and Music** (area) | 2,264 | |
| 811 | Synthesizers | 634 | 1,012 |
| 812 | Musical Instruments | 799 | 1,514 |
| 813 | Amplifiers and Audio Circuits | 279 | 778 |
| 814 | Speakers, Microphones and Acoustics | 180 | 478 |
| 815 | Digital Audio and DSP | 13 | 234 |
| 816 | Audio Players and Retro Audio | 279 | 707 |
| 817 | Chiptune and Circuit Bending | 80 | 197 |
| **820** | **Games** (area) | 1,888 | |
| 821 | Handheld Consoles | 237 | 443 |
| 822 | Console Hacks and Mods | 379 | 978 |
| 823 | Arcade Machines | 166 | 251 |
| 824 | Emulation and Retro Gaming | 425 | 912 |
| 825 | Pinball and Tabletop Games | 60 | 95 |
| 826 | Game Controllers | 423 | 783 |
| 827 | Homebrew Games | 198 | 544 |
| **830** | **Toys, Props and Seasonal** (area) | 1,296 | |
| 831 | Lego and Construction Toys | 201 | 409 |
| 832 | Toys and Blasters | 246 | 517 |
| 833 | RC Cars and Toys | 102 | 263 |
| 834 | Halloween | 233 | 302 |
| 835 | Christmas and Holidays | 154 | 266 |
| 836 | Movie and Sci-Fi Props | 142 | 309 |
| 837 | Cosplay and Costumes | 126 | 281 |
| 838 | Special Effects and Pyrotechnics | 92 | 161 |
| **840** | **Art** (area) | 811 | |
| 841 | Light Art | 180 | 447 |
| 842 | Kinetic Art | 153 | 274 |
| 843 | Interactive Installations | 259 | 480 |
| 844 | Circuit Sculpture and PCB Art | 88 | 136 |
| 845 | Generative and Plotter Art | 106 | 202 |
| 846 | Steampunk and Novelty Builds | 25 | 53 |

## 7. Magazine 8's store of podcast stories

The podcast's stories are filed in Sound and Play's hexadecimal pages after that magazine's own store: 25 pages for 6,232 stories (255 a page), in episode order. Each story is also a row in the lists of its subjects, its tags and its episode (121), so it is found wherever its subject is. Two episodes have no stories: Ep 256 (the anniversary special) and Ep 351 (from the Chaos Communication Congress) were not divided by the model in five attempts; each is to be one story, the whole episode (question 4).

## 8. The organisations, and where each is

Requirement H.6 item 2 asks for the archive organised in every way that makes sense. Each is a page or a list:

| By | Where | Count |
|---|---|---|
| Subject | Each subject's page, in its magazine | 266 subjects in 36 areas, 8 magazines |
| Date | 150 to 172, a page a year, a sub-page a day; 106 on this day; 101 and N02 the newest | 23 years |
| Writer | 130, and each writer's list | 182 writers |
| Series | 110, the nine largest at 111 to 125, the rest in lists | 35 series and more (`hackaday-site-structure.md` section 4) |
| Tag | 104, and each tag's list | 3,039 tags with lists; the rest through search |
| Hackaday's categories | Not a view: each category's posts are found under the subjects that cover them, and its name is searchable | 129 categories |
| Podcast episode | 121 | 397 episodes |
| Contest and event | 143 and 147 to 149 | The Prize 2014 to 2023, the challenges since 2013, Supercon |

Fun groupings (H.6 item 5) are left for later, as the owner said: they are to be built from tags and themes across the archive, and take the free pages of section 5 and N04 to N09 of each magazine.

## 9. What it uses of the standard

| What | Used | The standard's maximum |
|---|--:|--:|
| Magazines | 8 | 8 |
| Decimal pages | about 430 | 800 |
| Hexadecimal pages | about 372: 234 for articles' homes, 25 for stories', 113 for long lists | 1,222 |
| Sub-pages on the busiest page | 8,160 (a store page) | 8,190 |
| Sub-pages of the longest list | 356 (the busiest writer's) | 8,190 |

## 10. Requirement H.7: the ideas kept and the faults fixed

| Idea or fault (`hackaday-hierarchy.md` sections 2 and 3) | Where |
|---|---|
| Idea 1, magazines as subjects with one template | Sections 4 and 6 |
| Idea 2, a story as named sub-pages | Section 3: a home's places, named in row 1 |
| Ideas 3 and 4, pictures as mosaics, QR codes | Section 3, places 29 and 30 |
| Idea 5, the front page | 100 |
| Idea 6, features with a pulse | 112, 147, N04 to N09 |
| Idea 7, the people | 122, 123, 131 |
| Idea 8, the archive as play | 106, 107, 133, 136 |
| Idea 9, interaction | 105, 108, 134, 135 |
| Idea 10, lists done properly | Rule 3; 101, 103, 104, 109, 132 |
| Idea 11, Fastext and the header | Every page: the magazine, its list, next, home |
| Fault 1, a window not an archive | Every post has a home; every list holds the whole archive |
| Fault 2, numbers that move | Rule 1 |
| Fault 3, first-rule-wins subjects | Subjects from the readings; an article in every subject it belongs to |
| Fault 4, noisy tag pages | Merged tags; only tags on 20 posts or more have lists |
| Fault 5, magazine 1 a cupboard | Section 5, a decade a kind |
| Fault 6, comments take numbers | Rule 5 |
| Faults 7 and 8, no "by" | Section 8 |
| Fault 9, our own limits | Rule 6, section 9 |

## 11. Questions for the owner

1. **Homes and lists** (rules 2 and 3): an article has one home and is a linked row everywhere else, rather than copied into each place. Agree?
2. **Hexadecimal pages for storage** (rule 4): homes and the writers' and tags' lists on pages a viewer cannot key, reached by links. Agree?
3. **Writers' and tags' lists share pages** in fixed runs of places (section 5): stable and compact, but stepping past the end of one tag's run reaches the next tag's. Or one page each, for the busiest only?
4. **Ep 256 and Ep 351** as one story each, the whole episode (section 7)?
5. **The magazines' names and their areas** (section 6), and the two merges.

## 12. Not done

1. The cut points of a story fall on the start of a transcript line, within about 10 seconds of where the hosts turn to it; a cut can carry a few seconds of the story before.
2. The second subjects are looser than the main ones (the model adds "Arduino and AVR" to many posts that merely use one); a pass to check them is not done.
3. The page numbers of each magazine's features at N04 to N09, and the fun groupings.
4. The build (work step 13), after `STANDARD-LIMITS.md` section 7 and the comments design.
