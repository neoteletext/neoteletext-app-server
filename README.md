# teletext-server

The NeoTeletext server: a teletext application server that applications publish pages and streams to, and that sends each client, as 42-byte teletext packets, only what changed in the pages it shows.

The protocol it speaks is `~/devel/neoteletext-common/PROTOCOL.html`.

It imports the teletext it needs from `neoteletext-engine`, pinned at one commit, and never copies it. What the engine lacks for a server is written here, once.
