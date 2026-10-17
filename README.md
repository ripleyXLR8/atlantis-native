# Atlantis Native

A native, multiplatform port of **Atlantis: The Lost Tales** (Cryo
Interactive, 1997), written in Rust, for Windows, Linux, macOS and the web.

It is a translation of the Atlantis engine written for
[ScummVM](https://www.scummvm.org/)'s CryOmni3D engine, which was built by
reverse engineering the original game's executable. The goal is the
original game, pixel for pixel and sample for sample, with optional fixes of
the original's bugs.

**Status: phase 2 (scripts and dialogs) done.** A new game starts as the
original's does: the chapter script runs its intro (the music, the opening
cinematics with their sound, the first dialog line), then you can look
around and walk between the places of Atlantis, with the transition videos,
the animated place sprites and the ambient sounds. The chapter scripts run
as you move and click (their scans, timers, sounds, music and chapter
changes), and their dialogs play: the talking heads over the panorama at
each camera's field of view, the voices, the subtitles, the subject menu of
the characters you talk to, the object given shown under the heads, and the
game-over screens with their narrator. The 3D mouths of the talking heads
come with phase 4: until then the mouth area of a head is left as the video
has it. There are no inventory, menus or puzzles yet (a puzzle counts as
won).

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

Controls: move the mouse to look around (the pointer is captured by the
window, as the original takes it fullscreen), F5 / F6 / F7 / F8 turn and
tilt the view, click where the cursor animates to go there, click a
character to talk. A left click ends a dialog line; Escape ends a talking
head's line and skips the lines after it up to the next script command.
Escape in the panoramic view quits (the in-game menu comes with phase 3).

`atlantis-native --original /path/to/game` plays the original game, bug for
bug (see [Enhancements](#enhancements)).

`atlantis-native --headless /path/to/game [frames]` runs the engine without
a window, with no input and the sound mixed and dropped, and prints what it
did (the places, videos, sections, dialog lines and voices).

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

The engine is one async task: where the original's loops present a frame
and poll their events, the engine awaits the next frame, and the front end
runs it one step per displayed frame. The same code runs on desktop and in
the browser.

Where the original runs its scripts from navigation (arrival and departure
scans, click scans, timers, characters and objects clicked, puzzle zones,
the inventory bar and the menu), the engine calls the `ScriptHooks` trait
(`crates/atlantis-engine/src/hooks.rs`). The CON script interpreter
(`crates/atlantis-engine/src/con/`, `scripts.rs`) implements it; the dialog
lines of a section go to the `DialogPresenter` trait, which `Dialogs`
(`crates/atlantis-engine/src/dialog/`) implements: the INITDIAL table and
the camera choice, the view behind each line, the UBB talking-head videos
composited over the panorama, the voices, the subtitles
(`subtitles.rs`), the subject menu, the error box of a head that cannot be
opened and the game-over screen.

### Enhancements

Where the port may differ from the original by choice, `Options` holds an
`Enhancements` setting with three groups, those of the ScummVM port:

| Group | Default | What it changes |
|---|---|---|
| `minor_bug_fixes` | on | the music loops seamlessly (the original's streaming ring has audible seams); the SPF overlay of some talking heads (the guards' coif, the pilot's scarf) is drawn over the head (the original draws it where the next copy erases it) |
| `restored_content` | off | CHAPI063's timer lines of Anna can play (each condition starts from a clear key buffer); with the 2001 build, a second death in a section plays the next narrator line, as the 1999 build does |
| `ui_ux` | off | the error box of the seven lines whose talking head the original cannot open is not shown (the line is skipped either way) |

`--original` turns all three off. There is no settings screen yet.

### Subtitles

The original draws its subtitles with the system's Arial at 13 pixels, not
smoothed. The port draws them with Liberation Sans, which has Arial's
metrics, at the same size, rasterised by `ab_glyph` and thresholded to one
bit, so every platform draws the same pixels; the text bytes are read as
Windows-1252. The font is bundled in `assets/fonts`.

### The web build

`web/build.sh` builds it; the page asks for the game folder. The engine
reads the player's files through an asynchronous loader that fetches each
archive member before it is read, so the browser runs the same game as the
desktop. It has been built but not tried in a browser yet. A browser grants
the pointer capture only on a click: until then the view turns with the
pointer's own motion. Sound starts on the first click.

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

The subtitle font, Liberation Sans Regular
(`assets/fonts/LiberationSans-Regular.ttf`, Copyright (c) 2010 Google
Corporation, Copyright (c) 2012 Red Hat, Inc.), is licensed under the SIL
Open Font License, version 1.1 (see `assets/fonts/LICENSE-LiberationSans`).

*Atlantis: The Lost Tales* is a trademark of its owners. This project is not
affiliated with them.
