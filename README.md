# Atlantis Native

A native, multiplatform port of **Atlantis: The Lost Tales** (Cryo
Interactive, 1997), written in Rust, for Windows, Linux, macOS and the web.

It is a translation of the Atlantis engine written for
[ScummVM](https://www.scummvm.org/)'s CryOmni3D engine, which was built by
reverse engineering the original game's executable. The goal is the
original game, pixel for pixel and sample for sample, with optional fixes of
the original's bugs.

**Status: playable from start to end.** The game starts as the
original's does: the Cryo logo and the pre-intro, then the player screen,
where you create, choose or delete a player. The chapter scripts run their
cinematics, dialogs, music and sounds as you look around and walk between
the places of Atlantis; the talking heads of the dialogs move their 3D
mouths in time with the voices. The inventory, the characters who follow
you, the in-game menu (load a chapter, the options, quit), the checkpoints
written at every chapter change, the game overs, all seventeen puzzles and
the end with its credits play as in the original.

## Downloads

Builds of the latest commit are on the
[nightly release](https://github.com/ripleyXLR8/atlantis-native/releases/tag/nightly):

| File | System |
|---|---|
| `atlantis-native-windows-x64.zip` | Windows 10 and later, 64-bit |
| `atlantis-native-macos-arm64.dmg` | macOS 11 and later, Apple Silicon |
| `atlantis-native-linux-x64.tar.gz` | Linux, x86-64 (glibc 2.35 and later; ALSA for the sound) |
| `atlantis-native-web.zip` | the web build: a static site to serve over HTTP |

The macOS build is signed ad hoc only: the first time, open it with a right
click and Open (or allow it in System Settings, Privacy & Security). On
Linux, the folder picker is the desktop's (through the XDG desktop portal,
or `zenity`).

## You need the game

Atlantis Native contains no game data. You need your own copy of the game:

- the English CD release (2001 build): `atlantis.exe` and `BIGCD1.BIG` ..
  `BIGCD4.BIG`;
- or the GOG.com release (English, French, German, Spanish, Italian): its
  installation folder.

## Playing

Double-click `atlantis-native` (on macOS, `Atlantis Native`). The first
time, it asks for the game folder in the system's folder picker: the folder
with `atlantis.exe` and the `BIGCD*.BIG` files, or the GOG.com installation
folder. It remembers the folder (`game-folder.txt` in the per-user folder,
see [Players, checkpoints and settings](#players-checkpoints-and-settings))
and asks again only if the folder no longer holds the game. Errors are
shown in a message box. When it is not started from a terminal, the log
goes to `atlantis-native.log` in the same folder (rewritten at every start;
set `RUST_LOG` to send it to the terminal's standard error).

From a terminal, a game folder given on the command line is used instead
(it is not remembered):

```sh
atlantis-native /path/to/game
```

## Building

```sh
cargo build --release
cargo run --release -- /path/to/game
```

On Linux, building needs the ALSA headers (`libasound2-dev` or
`alsa-lib-devel`); nothing else to link (the windowing libraries, the
desktop portal's libdbus are loaded at run time).

## Controls

Move the mouse to look around (the pointer is captured by the
window, as the original takes it fullscreen), F5 / F6 / F7 / F8 turn and
tilt the view, click where the cursor animates to go there, click a
character to talk. A left click ends a dialog line; Escape ends a talking
head's line and skips the lines after it up to the next script command.
A right click opens the inventory bar (its edges scroll it); Escape in the
panoramic view opens the in-game menu. On the player screen, click the
last row to type a new name and press Enter or OK; Escape there quits (the
port's way out; the original has none).

## Command line

The game folder can be left out of each of these: the remembered one is
used, or the picker asks for it (except with `--headless`). The Windows
release build has no console: redirect its output to a file
(`atlantis-native.exe --headless C:\game > out.txt`) to read it.

`atlantis-native --new-game /path/to/game` skips the start-up videos and
the player screen.

`atlantis-native --original /path/to/game` plays the original game, bug for
bug (see [Enhancements](#enhancements)).

`atlantis-native --headless /path/to/game [frames]` runs a new game without
a window, with no input and the sound mixed and dropped, and prints what it
did (the places, videos, sections, dialog lines and voices).

`atlantis-native --puzzle <n> [--object <id>] /path/to/game` (a testing
aid, with or without `--headless`) plays puzzle `n` (the scripts'
StartPuzzle number, 0..16) at once, with object `id` in hand (322, the
missing tile, for the sliding puzzles 6, 7 and 14), reports its result,
then starts a new game.

## Players, checkpoints and settings

There are no free saves: as in the original, the game saves itself at
every chapter change, in the original's own files: `players.atl` (the
player list) and `p<letter><chapter>.gam` (one checkpoint per chapter and
player, the original's 2639-byte format). The load picker of the in-game
menu lists the current player's checkpoints by their chapter names. The
files are kept in a per-user folder, one per game build:

| System | Folder |
|---|---|
| Linux and other Unix systems | `$XDG_DATA_HOME/atlantis-native/<build>` (by default `~/.local/share/atlantis-native/<build>`) |
| macOS | `~/Library/Application Support/atlantis-native/<build>` |
| Windows | `%APPDATA%\atlantis-native\<build>` |

(`game-folder.txt` and `atlantis-native.log` are in the `atlantis-native`
folder itself.)

On its first start with a game whose folder holds an original
`players.atl` (an installation the original game played), the port copies
it and its checkpoints into that folder; the game folder is only read.
Files written by the port can be copied back for the original game.

Deleting a player deletes that player's own checkpoints; the players after
it move up with their files. (The original deletes the files of the letter
of the row, which after an earlier deletion can be another player's and
leaves the deleted player's files behind.)

The same folder holds `settings.ini`: the options of the game's menus
(subtitles, OMNI 3D mode, the master, music, effects and voices volumes,
which the original also keeps in each checkpoint and sets again when one
is loaded) and the [enhancements](#enhancements), one `name = on|off` line
each.

The web build keeps its players, checkpoints and settings in the
browser's storage (`localStorage`, one entry per file, kept per site and
per browser). It too takes over the `players.atl` and checkpoints of the
picked folder on its first start.

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
the inventory bar's timers, a guest's portrait clicked), the engine calls
the `ScriptHooks` trait
(`crates/atlantis-engine/src/hooks.rs`). The CON script interpreter
(`crates/atlantis-engine/src/con/`, `scripts.rs`) implements it; the dialog
lines of a section go to the `DialogPresenter` trait, which `Dialogs`
(`crates/atlantis-engine/src/dialog/`) implements: the INITDIAL table and
the camera choice, the view behind each line, the UBB talking-head videos
composited over the panorama, the voices, the subtitles
(`subtitles.rs`), the subject menu, the error box of a head that cannot be
opened and the game-over screen.

The 3D talking heads are in `crates/atlantis-engine/src/head/`: the dialog
video leaves the speaker's mouth empty, and the line player draws a
textured 3D lower face there every frame, posed by the head animation at
the video's frame and shaped by the line's lip-sync track at the voice's
time (`mod.rs`: animation, camera, tree walk, culling and projection;
`raster.rs`: the original's span rasteriser, flat, affine and perspective
texture fills). The files (`.3DC` meshes, `.3DA` animations and mouth
shapes, `.S3D` tables and cameras, `.SYC` lip-sync tracks, `.3DM` texture
maps) are parsed in `crates/atlantis-data/src/head.rs`. The arithmetic is
the original's, operation for operation (32-bit fixed point, x87 doubles
with float stores, the transcendental functions of the `libm` crate), so
the heads are drawn pixel for pixel as the original draws them, on every
platform. Three paths of the original that no shipped head and camera
reach are not ported (they log a warning): the clipping of faces at the
near plane, of edges above the top of the screen, and the slerp of
opposite quaternions.

The 2D interface is in `crates/atlantis-engine/src/ui/`: the menu files'
walk and the menu fonts (`layout.rs`), the menus, the player screen, the
load picker, the credits and the end (`menus.rs`, `credits.rs`), the
inventory bar, the object picked up, bow mode and the guests' portraits
(`bar.rs`, `mod.rs`). The players and checkpoints are in
`crates/atlantis-engine/src/checkpoint.rs`, their file formats in
`crates/atlantis-data/src/saves.rs`, the save folder and the settings file
in `crates/atlantis-engine/src/persist.rs`.

The puzzles are in `crates/atlantis-engine/src/puzzle/`. `mod.rs` holds the
dispatcher (StartPuzzle n run by zone 1000 + n: its checks, the result
set in FinPuzzle), the cursor animation and the random generators the
puzzles share, and what the original keeps from one puzzle to the next;
`assets.rs` the PUZZLES\ files, the work screen, the input read at the
frame boundaries and the puzzle samples. Each puzzle is one file: its
state and loop, translated line for line, behind a trait of its own for
the input, drawing and sounds, which the engine implements and the tests
replay the original's runs through, pass by pass (`throne.rs`,
`taquin.rs`, `meta.rs`, `hexa.rs`, `crab.rs`, `muria.rs`).

### Enhancements

Where the port may differ from the original by choice, `Options` holds an
`Enhancements` setting with three groups, those of the ScummVM port:

| Group | Default | What it changes |
|---|---|---|
| `minor_bug_fixes` | on | the music loops seamlessly (the original's streaming ring has audible seams); the SPF overlay of some talking heads (the guards' coif, the pilot's scarf) is drawn over the head (the original draws it where the next copy erases it) |
| `restored_content` | off | CHAPI063's timer lines of Anna can play (each condition starts from a clear key buffer); with the 2001 build, a second death in a section plays the next narrator line, as the 1999 build does |
| `ui_ux` | off | the error box of the seven lines whose talking head the original cannot open is not shown (the line is skipped either way) |

`--original` turns all three off. Otherwise they are read from
`settings.ini` (see [Players, checkpoints and settings](#players-checkpoints-and-settings)),
which the first start writes with the defaults; there is no settings
screen.

### Subtitles

The original draws its subtitles with the system's Arial at 13 pixels, not
smoothed. The port draws them with Liberation Sans, which has Arial's
metrics, at the same size, rasterised by `ab_glyph` and thresholded to one
bit, so every platform draws the same pixels; the text bytes are read as
Windows-1252. The font is bundled in `assets/fonts`.

### The web build

`web/build.sh` builds it (`atlantis-native-web.zip` on the nightly
release holds the result); serve the folder over HTTP (for example
`python3 -m http.server -d web 8000`), the page asks for the game folder. The engine
reads the player's files through an asynchronous loader that fetches each
archive member before it is read, so the browser runs the same game as the
desktop. A browser grants
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
| 6 | packages for each platform, web build, folder picker, saves in the browser |

The web build has been built in CI but little tried in browsers. There is
no application icon yet (the game's own pictures are not used: they are
not ours to distribute).

## Known differences from the original

- The window: the game runs in a resizable window (the picture scaled,
  in its 4:3 proportions) rather than fullscreen at 640x480.
- The saves are in a per-user folder, not in the game folder (see above);
  deleting a player deletes its own checkpoints.
- The subtitles are drawn with Liberation Sans, which has the metrics of
  the original's Arial (see [Subtitles](#subtitles)).
- The engine runs at a steady 60 steps per second; the original's loops
  run as fast as the machine allows.
- The [enhancements](#enhancements) on by default: the music loops
  seamlessly; the SPF overlays of some talking heads are drawn over the
  head. `--original` turns them off.
- Escape on the player screen quits.
- Three paths of the 3D talking heads that no shipped head reaches are
  not ported (see the layout notes below).

## Licence

GNU General Public License, version 3 or later (see `LICENSE`). The code is
derived from ScummVM's CryOmni3D engine, Copyright (C) the ScummVM team.

The subtitle font, Liberation Sans Regular
(`assets/fonts/LiberationSans-Regular.ttf`, Copyright (c) 2010 Google
Corporation, Copyright (c) 2012 Red Hat, Inc.), is licensed under the SIL
Open Font License, version 1.1 (see `assets/fonts/LICENSE-LiberationSans`).

*Atlantis: The Lost Tales* is a trademark of its owners. This project is not
affiliated with them.
