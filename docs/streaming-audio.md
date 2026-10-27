# teletext-server: streaming audio

| | |
|---|---|
| **Asked** | By the owner on 25 October 2026: "could you make some sort of server application that plays internet radio stations?", then: "use symphonia, include it in the licence credits for the server. what i want is the capability to do streaming audio transcoding built in to the server design that how would it work what would it look like to the programmer. also design the same so that we can stream internet radio stations or stream podcasts using http byte ranges". Station list: radio-browser.info. |
| **State** | Design, 25 October 2026. Nothing built. Section 10 lists what the owner must decide. |
| **Measured** | Section 7: symphonia 0.6.1 on eight real streams, recorded the same day. |

## 1. What it is

Transcoding becomes a service of the server, like publishing a page. An application names a source by URL. The server then does the work:
1. fetches it;
2. decodes MP3, AAC or FLAC;
3. mixes it to mono and resamples it to the audio profile's 16 kHz;
4. encodes it as IMA ADPCM (the engine's encoder);
5. sends it to every session the application says is listening.

The application never touches a byte of audio. It hears what is playing (the station's title, levels and position) and draws pages from that.

There are two kinds of source:
- **Live:** one stream, the same moment for every listener. Examples are an Icecast or SHOUTcast station, an HLS station, or samples the application makes itself (the showcase's radio). It is transcoded once, however many listen.
- **On demand:** a file each listener hears from their own point, such as a podcast episode. It is fetched by HTTP byte ranges, transcoded once into a cache, and served to each listener from their own position.

## 2. The pipeline

```
 URL ──► fetch ──► unwrap ─────────► decode ──► mono ──► resample ──► IMA ADPCM ──► blocks
         HTTP(S)   ICY metadata out  symphonia  mix      to 16 kHz    engine        64 ms each
         ranges    HLS playlists,                                     encoder
         redirects TS → ADTS                                                          │
                                                                                      ▼
                     title, levels, position ──► application        blocks ──► stream ring ──► connections
```

- **The transcoder is sans I/O, like the runtime.** `teletext-transcode` takes bytes in and gives encoded blocks and events out. It holds no socket, clock or thread. A host feeds it what it fetched. It builds for `wasm32-unknown-unknown` as well: symphonia 0.6.1, with the codecs needed, compiled for that target on 25 October 2026.
- **The fetcher is the host's.** On the native host it is a task, or a worker process (section 8), holding the HTTP connection.
- **Blocks go to listeners as pages go to viewers** (decision 7). The runtime never touches a listener's audio.
  - A live stream is a ring of encoded blocks with a sequence number.
  - Each connection's own task catches up from the last block it sent, up to the receiver's lead.
  - A slow connection skips to the newest blocks; no queue grows.
- **The output is the engine's audio profile, unchanged:**
  - IMA ADPCM at 16 kHz, mono, in 512-byte blocks;
  - 64 kbit/s per listener;
  - the live-sound records the showcase radio sends today.

  The browser needs no change.

## 3. What it looks like to the programmer

### 3.1 The interface

```rust
/// Where a stream's sound comes from.
pub enum AudioSource<'a> {
    /// An endless stream, one moment for everyone: Icecast, SHOUTcast or HLS.
    Live { url: &'a str },
    /// A file each listener hears from their own point: a podcast episode,
    /// fetched by byte ranges.
    OnDemand { url: &'a str },
    /// Samples the application makes itself (`Context::audio_push`).
    Generated,
}

impl Context<'_> {
    /// Opens a stream, bound to `page`: a listener's receiver fades it out
    /// when they leave that page (the engine's `audio.md` section 7.3).
    pub fn audio_open(&mut self, source: AudioSource<'_>, page: u16) -> StreamId;
    /// `session` hears the stream: live from now, on demand from `from_ms`.
    pub fn audio_listen(&mut self, session: SessionId, stream: StreamId, from_ms: u64);
    pub fn audio_leave(&mut self, session: SessionId, stream: StreamId);
    /// On demand only: each listener's own position.
    pub fn audio_seek(&mut self, session: SessionId, stream: StreamId, to_ms: u64);
    pub fn audio_pause(&mut self, session: SessionId, stream: StreamId, paused: bool);
    /// Generated only: 16-bit mono samples at 16 kHz, appended.
    pub fn audio_push(&mut self, stream: StreamId, samples: &[i16]);
    pub fn audio_close(&mut self, stream: StreamId);
}

pub trait Application: Send {
    // ...the calls of today, and one more:
    fn audio(&mut self, _stream: StreamId, _event: &AudioEvent<'_>, _context: &mut Context<'_>) {}
}

pub enum AudioEvent<'a> {
    /// The source is decoding: what it is.
    Opened { codec: &'a str, kilobits: u32, rate: u32, channels: u8, name: Option<&'a str> },
    /// What is playing now: an ICY `StreamTitle`, HLS ID3 tags, or a file's tags.
    Title(&'a str),
    /// Four times a second, for a page's meters: 0 to 255.
    Levels { peak: u8, loudness: u8 },
    /// On demand, once a second for each listener.
    Position { session: SessionId, at_ms: u64, length_ms: Option<u64> },
    Buffering,
    Playing,
    Ended,
    Failed(AudioFault),
}
```

The runtime's event and effect lists (architecture section 3.1) each grow by one:
- `AudioOpen(stream, source)` and `AudioClose(stream)` to the host;
- `Audio(stream, event)` from the host.

Listening and leaving stay inside the runtime and the hub. The host's transcoder writes blocks straight into the stream's ring, so a block never passes through the runtime.

### 3.2 A radio page

```rust
fn input(&mut self, session: SessionId, page: u16, input: &Input<'_>, context: &mut Context<'_>) {
    if input.pressed(Named::Enter) {
        let station = self.list.selected(session);            // from radio-browser (section 6)
        let stream = *self.open.entry(station.uuid).or_insert_with(|| {
            context.audio_open(AudioSource::Live { url: &station.url }, page)
        });
        context.audio_listen(session, stream, 0);
        self.tuned.insert(session, stream);
        self.redraw(session, context);
    }
}

fn audio(&mut self, stream: StreamId, event: &AudioEvent<'_>, context: &mut Context<'_>) {
    if let AudioEvent::Title(title) = event {
        self.titles.insert(stream, title.to_string());
        // Every viewer tuned to it sees the new title at once.
        for session in self.tuned_to(stream) { self.redraw(session, context); }
    }
}
```

The application closes a stream when its last listener leaves. One open stream serves every listener, so 10,000 people on one station cost one transcode.

### 3.3 A podcast page

```rust
let episode = context.audio_open(AudioSource::OnDemand { url: &item.enclosure }, page);
context.audio_listen(session, episode, resume_ms);     // where this viewer left off
// The keys: Left and Right skip 15 s, Space pauses.
context.audio_seek(session, episode, at.saturating_sub(15_000));
```

`AudioEvent::Position` drives each viewer's progress bar on their own page.

### 3.4 The showcase radio, rewritten

Today `audio/radio.rs` does all of this itself, in about 400 lines:
- keeps the ring of recent blocks;
- tracks who joined;
- sends the object, the PLAY and the catch-up;
- tops the stream up four times a second;
- numbers generations.

With `AudioSource::Generated` it only renders samples:

```rust
fn wake(&mut self, context: &mut Context<'_>) {
    let samples = self.tracker.render(self.next, TICK_SAMPLES);
    context.audio_push(self.stream, &samples);
    context.wake_in(TICK_MS);
}
```

Its block-numbering and join logic moves into the runtime, where every stream shares it.

## 4. Live sources

- **Icecast and SHOUTcast** are an endless HTTP body.
  - The request asks `Icy-MetaData: 1`.
  - The answer's `icy-metaint` says how many audio bytes come between metadata blocks.
  - Each block is a length byte (times 16) and text such as `StreamTitle='Artist - Song';`.
  - The unwrapper strips the blocks before decoding and reports the title.
  - Of the streams recorded (section 7), the SHOUTcast 2 server answered `HTTP/1.0 200 OK`. Only the oldest SHOUTcast 1 servers answer `ICY 200 OK`, which a strict HTTP parser refuses. Section 10 item 2.
- **HLS** (`.m3u8`) is a playlist of segments, re-read every target duration.
  - RTL's segments were MPEG-TS holding ADTS (stream type 0x0F) and an ID3 metadata stream (0x15), which carries the title.
  - symphonia has no MPEG-TS reader. A small one (program table, then the audio stream's PES payloads) feeds its ADTS reader, and decoded 17 s cleanly.
  - Segments in fragmented MP4 (`#EXT-X-MAP`) go to symphonia's MP4 reader. These were not met or tested.
- **Reconnecting:** a dropped stream is retried with backoff. Listeners hear silence while it buffers, and the application is told `Buffering`. After a set time it is told `Failed`.
- **Fetched only while someone listens** (`SERVER.md`'s rule that nothing is made for nobody). The last listener leaving closes the connection after a short grace, so a tab switch does not restart it.

## 5. On-demand sources: podcasts by byte ranges

- **The source is a seekable byte source over HTTP ranges.**
  - It reads `Range: bytes=a-b` chunks of 256 KB as symphonia's reader asks.
  - It reports itself seekable, with the length from `Content-Range`.
  - symphonia can then seek a VBR MP3 by its Xing table and read an M4A whose index (`moov`) is at the end of the file.
- **Measured on 25 October 2026, three podcast feeds** (BBC, NPR, a Simplecast host):
  - every one answered a range with `206` and `Accept-Ranges: bytes`;
  - each came after 3 to 6 redirects through tracking services;
  - the files were larger than the feeds declared (BBC 15.0 MB against 13.4 MB). That is ads inserted when the file is requested;
  - there was no `ETag`, and `Last-Modified` was the time of the request.
- **So the design:**
  1. Follow the redirects once per episode. Count them (at most 10), and check every hop (section 9).
  2. Fetch every range from the final address.
  3. Check that every answer's total length matches the first. A changed length means a different file: start the episode's cache again.
  4. A refused final address (signed addresses expire) is resolved again from the feed's address.
- **Transcode once, into a cache.** Blocks are kept by index: block *n* starts at *n* × 64 ms.
  - A listener at 12:00 reads from block 11,250.
  - A seek past what is cached starts transcoding there (symphonia's `seek`), leaving a gap that fills later.
  - Fetching runs a few minutes ahead of the furthest listener, not to the end.
  - The cache is 28.8 MB an hour of audio, kept in memory or on disk under a size limit, least recently used first.
- **Each listener's blocks are framed for them.** A live record carries its stream's generation and block number, so a listener with their own position gets their own numbering. That means copying 512 bytes into packets per block sent. Live listeners share one framed copy.

## 6. Stations: radio-browser.info

The directory held 63,061 stations on 25 October 2026. Its rules (api.radio-browser.info):
- **Find a server through DNS:**
  - look up `all.api.radio-browser.info`;
  - shuffle the addresses and try the next on failure;
  - never hard-code one server.
- **Identify the client** with a plain User-Agent such as `teletext-server/0.1`.
- **Count each listen** with `/json/url/{stationuuid}` when a viewer tunes, which keeps the directory's popularity honest.
- **Data:** free to use, mirror and fork. No licence is stated beyond that.

Search by name, country, language, tag or popularity. Then:
- keep `lastcheckok` stations;
- play `url_resolved`;
- offer only codecs the transcoder decodes.

Results are cached for an hour, so 1,000 viewers browsing cost one query.

The directory's codecs (`/json/codecs`, the same day):

| Codec | Stations | Share | Decoded |
|---|---|---|---|
| MP3 | 46,686 | 67.1% | Yes |
| AAC+ (HE-AAC) | 10,088 | 14.5% | Yes, as its core (section 7) |
| AAC | 9,556 | 13.7% | Yes |
| Unknown | 2,280 | 3.3% | When it turns out to be one of these |
| OGG (Opus and Vorbis) | 781 | 1.1% | Vorbis yes; **Opus no** (section 10 item 1) |
| FLAC | 5 | — | Yes |

About 95% of stations are playable with symphonia alone.

## 7. Measured

symphonia 0.6.1 was built with every codec and fed eight recordings, 17–35 s each, read as an unseekable stream, as a live station arrives. Each was then:
1. mixed to mono;
2. resampled to 16 kHz (a windowed-sinc filter written for the probe);
3. encoded with the engine's IMA ADPCM encoder.

Costs are milliseconds of one core per second of audio, on the development Mac:

| Station (format) | Decoded as | Decode | Resample | Encode | Total |
|---|---|---|---|---|---|
| France Info (MP3 128k) | 48 kHz stereo | 0.92 | 13.45 | 0.24 | 1.5% of a core |
| Europe 1 (AAC-LC 128k) | 48 kHz stereo | 3.70 | 13.38 | 0.22 | 1.7% |
| Fox News Radio (HE-AAC 64k) | 22.05 kHz stereo | 1.54 | 7.24 | 0.22 | 0.9% |
| Arabesk FM (HE-AAC v2 64k) | 22.05 kHz mono | 0.32 | 7.47 | 0.23 | 0.8% |
| RTL (HLS, HE-AAC v2 64k, TS) | 24 kHz mono | 0.41 | 6.83 | 0.21 | 0.7% |
| Rondo Klasu (FLAC) | 44.1 kHz stereo | 0.88 | 13.48 | 0.21 | 1.5% |
| Deutschlandfunk, Classic Vinyl (Opus) | — | refused: "unsupported audio codec" | | | |

- **HE-AAC without its high band is enough here.**
  - symphonia decodes the AAC-LC core and skips the spectral band replication (SBR) that rebuilds the top band.
  - At 64 kbit/s the core runs at 22–24 kHz, so it carries up to 11–12 kHz, beyond the 8 kHz a 16 kHz output can hold.
  - The share of energy from 6.5 to 7.2 kHz after resampling:

    | Stream | 6.5–7.2 kHz |
    |---|---|
    | MP3 | −24.2 dB |
    | AAC-LC | −28.3 dB |
    | The three HE-AAC | −22.8, −23.4 and −27.7 dB |

    Nothing is missing in the band we keep.
  - HE-AAC v2's parametric stereo is dropped too, which costs nothing in mono.
  - Not measured: the 24–32 kbit/s HE-AAC stations, whose core may stop below 8 kHz.
- **The resampler is most of the cost.** The probe's filter computes its sine and cosine for every tap. A precomputed polyphase table should cost several times less; not yet measured. Decoding is 0.3–3.7 ms per second of audio.
- **Memory:** a transcoding process peaked at 3.2–3.6 MB resident on 2-second inputs (macOS; Linux not measured).

## 8. Scale, and whether a separate process scales

**The work grows with the number of stations playing, not the number of listeners.** One station heard by 100,000 people is one fetch, one decode and one encode. The 100,000 connections read the same encoded blocks, exactly as they read a shared page (decision 7, measured at 100,000 connections). Sending is 64 kbit/s per listener, so a busy station is limited by the network (6.4 Gbit/s for 100,000 listeners), not the transcoder.

What each station playing costs, from section 7:

| Stations playing at once | One process each | Tasks in a few worker processes |
|---|---|---|
| 10 | 36 MB, 0.15 of a core | about the same |
| 100 | 360 MB, 1.5 cores | the CPU is the same; memory a little less |
| 1,000 | 3.6 GB, 15 cores | the CPU is the same; memory around 1–2 GB, not measured |

- **A separate process does scale,** because a process costs about 3.5 MB on top of work that costs 1–2% of a core anyway. At 1,000 stations the 15 cores matter far more than the 3.6 GB. A cheaper resampler (section 7) lowers both columns.
- **Why a process at all:** the transcoder parses untrusted input from anywhere on the internet with a large decoder. The browser runs its decoders in confined processes for the same reason (decision 4 there). In a separate, confined process:
  - a decoder fault ends one process, not the server and its 100,000 connections;
  - an exploited decoder reaches nothing.
- **Recommended: a pool of confined worker processes,** one per core by default, each transcoding many streams as tasks.
  - **Isolation:** a crash ends one worker's streams, which restart in another.
  - **Cost:** the memory of tasks, not processes.
  - **Small installations:** one process per stream is simpler, and fine up to a few hundred streams.
  - **Each worker:** may connect out to the internet, but never to private addresses (section 9). It has no file system beyond its on-disk cache, and talks to the server over a pipe.
  - **The pipe** carries encoded blocks and events, framed with the engine's length-prefixed records. The server writes the blocks into the stream's ring.

## 9. Security

The URLs come from a public directory anyone can add to, and from podcast feeds. Every byte fetched is hostile.

- **No requests to private addresses (server-side request forgery).** After every DNS answer and every redirect, refuse:
  - loopback, private and shared ranges (10/8, 172.16/12, 192.168/16, 100.64/10);
  - link-local (169.254/16, including cloud metadata at 169.254.169.254);
  - unique-local and other non-global IPv6, and multicast.

  Otherwise a station could make the server fetch inside its own network. Ports 80, 443 and the directory's stream ports are allowed. The redirect chain is limited to 10 hops.
- **Every length is bounded:**
  - response heads;
  - ICY metadata, at most 4080 bytes by its own format;
  - playlists and segment counts;
  - the bitrate taken in, which refuses FLAC at 1.5 Mbit/s where a station's share would exceed it;
  - the time without data before a stream counts as failed;
  - streams open at once, per application and per server.
- **Metadata is untrusted text.** A title is cut to the page's width and mapped to the page's character set by the application, like anything typed (INPUT.md section 9.5). It is never markup.
- **The decoder runs confined** (section 8), with fuzz targets for:
  - the ICY unwrapper;
  - the TS reader;
  - the playlist reader;
  - the range source's bookkeeping.

## 10. Decisions for the owner

1. **Opus (1.1% of stations, with Vorbis).** symphonia 0.6.1 has no Opus decoder. Options:
   - leave Opus stations out, which is the recommendation for now;
   - `symphonia-adapter-libopus` 0.3.0, which links the C libopus (BSD-3-Clause);
   - one of the young pure-Rust decoders (`opus-decoder` 0.1.1, `rusty-opus` 1.0.2, `opus-rs` 0.1.39), untested here.
2. **The HTTP client for fetching.**
   - The engine's `neoteletext-web` has a strict, sans-I/O HTTP/1.1 client (its own parser, bounded heads, chunked framing). Using it keeps one HTTP implementation, as decision 1 wants. It needs these engine changes: an endless body, the `Range` and `Icy-MetaData` headers, and audio media types.
   - `hyper`'s client (already approved, with `hyper-util`) works today.
   - Recommended: the engine's, extended by its own process.
   - Either refuses SHOUTcast 1's `ICY 200 OK`.
3. **A worker pool or a process per stream** (section 8). Recommended: the pool.
4. **Confining the workers.** The browser's `confine` crate does Landlock, seccomp and network namespaces on Linux, and its equivalents on macOS and Windows. Decision 1 imports only from the engine, so it would move to the engine first, or be imported from the browser as an exception.
5. **Where podcasts are found:**
   - a feed address the viewer types;
   - Apple's podcast search (no key);
   - Podcast Index (free, with an API key in `~/devel/credentials/`).
6. **Rights.** Relaying a station or a podcast to many listeners is retransmission. Many stations' and podcasts' terms forbid it, and ads inserted per request would be relayed to people they were not counted for. On the LAN for testing, this matters little. A public server needs a policy, for example only stations that allow relaying, or each listener fetching for themselves. That is the owner's call.
7. **The output rate.** 16 kHz (64 kbit/s) is the profile's wideband rate and suits music. 8 kHz (32 kbit/s) halves the bandwidth and suits speech-only stations and podcasts. It could be chosen per stream.

## 11. Not known yet

- HE-AAC below 48 kbit/s, and HLS in fragmented MP4: not tested.
- A precomputed resampler's cost, and a transcoding process's memory on Linux: not measured.
- How the browser's live sound treats block numbers that wrap after 65,536 blocks (70 minutes), and a stream that starts at a block other than 0: to be checked against the engine's `audio.md` section 8.3.
- The engine's live encoder restarts its step size on every call, costing 2–7 dB at each restart (`TECH-DEBT.md`). A transcoder that encodes block by block hits it every block. It is the engine change already recorded, now needed.
