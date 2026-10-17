# Atlantis Native

A native, multiplatform port of **Atlantis: The Lost Tales** (Cryo
Interactive, 1997), written in Rust, for Windows, Linux, macOS and the web.

It is a translation of the Atlantis engine written for
[ScummVM](https://www.scummvm.org/)'s CryOmni3D engine, which was built by
reverse engineering the original game's executable. The goal is the
original game, pixel for pixel and sample for sample, with optional fixes of
the original's bugs.

**Status: early work (phase 0).** Nothing is playable yet.

## You need the game

Atlantis Native contains no game data. You need your own copy of the game:

- the English CD release (2001 build): `atlantis.exe` and `BIGCD1.BIG` ..
  `BIGCD4.BIG`;
- or the GOG.com release (English, French, German, Spanish, Italian): its
  installation folder.

## Building

```sh
cargo build --release
cargo run --release -- /path/to/game
```

Tests that need the game files read the game directory from the
`ATLANTIS_DATA` environment variable and are skipped without it:

```sh
ATLANTIS_DATA=/path/to/game cargo test --release
```

The expected values in the tests are digests and numbers computed by the
original game's own code; they contain no game data.

## Layout

| Crate | Content |
|---|---|
| `atlantis-data` | BigFile archives, executable tables, game detection |
| `atlantis-codecs` | HNM6 panoramas and videos, APC sound |
| `atlantis-engine` | the game, independent of any platform: input events in, 640x480 frames and audio out |
| `atlantis-app` | the window, input and audio (winit, softbuffer, cpal) |

Comments cite addresses in the original `atlantis.exe` (`FUN_0042844c`,
`0x00496570`, ...) so that each behaviour can be traced to the original code.

## Roadmap

| Phase | Content |
|---|---|
| 0 | archives, executable tables, detection; one panorama in a window and in the browser |
| 1 | navigation: places, zones, cursor, sprites, transition videos, ambient sounds |
| 2 | scripts, dialogs, subtitles, voices, music |
| 3 | inventory, menus, players and checkpoints |
| 4 | 3D talking heads |
| 5 | the 17 puzzles |
| 6 | packages for each platform, web build |

## Licence

GNU General Public License, version 3 or later (see `LICENSE`). The code is
derived from ScummVM's CryOmni3D engine, Copyright (C) the ScummVM team.

*Atlantis: The Lost Tales* is a trademark of its owners. This project is not
affiliated with them.
