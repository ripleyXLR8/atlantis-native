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

F10 opens the port's options menu, over whatever the game shows (the game
pauses, its sound too): the graphics, Original or HD (see
[HD mode](#hd-mode)), the HD mode's picture options ("Panorama filter",
"Panorama sharpening", "Interface", "Sharp subtitles"; greyed in the
Original graphics), and the three groups of [enhancements](#enhancements),
"Fix original bugs", "Restored content" and "Modern UI/UX", on or off.
The foot of the menu shows the game's release (e.g. "GOG.com, French (1999
build)", also in the window's title) and the port's version. A change applies at once (the music mode from the next track) and is saved
in `settings.ini`. Up / Down choose an entry, Enter, Space, Left / Right
or a click change it; Escape, F10, Close or a click outside the menu
closes it. F9 shows a diagnostic overlay in the panoramic view (the
variables the scripts test).

## Command line

The game folder can be left out of each of these: the remembered one is
used, or the picker asks for it (except with `--headless`). The Windows
release build has no console: redirect its output to a file
(`atlantis-native.exe --headless C:\game > out.txt`) to read it.

`atlantis-native --new-game /path/to/game` skips the start-up videos and
the player screen.

`atlantis-native --original /path/to/game` plays the original game, bug for
bug (see [Enhancements](#enhancements)).

`atlantis-native --hd /path/to/game` starts in the [HD mode](#hd-mode),
whatever `settings.ini` says.

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
each. It also keeps `music_on_disk`, the music tracks the original game
would have copied to its folder (it reads them from the archives here),
so that this set outlives the session as the original's copies do,
`graphics = original|hd`, the graphics mode of the options menu (F10;
`original` by default), and the HD mode's picture options (see
[HD mode](#hd-mode)): `hd_panorama = bicubic|bilinear`,
`hd_sharpen = on|off`, `hd_interface = smooth|pixels` and
`hd_sharp_subtitles = on|off` (by default `bicubic`, `on`, `smooth`,
`on`).

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
which the first start writes with the defaults; the enhancements have no
settings screen.

### HD mode

The original draws everything at 640x480, and the port shows it scaled to
the window (nearest, in its 4:3 proportions). In the HD mode (options menu,
F10, or `--hd`) the picture is made at the window's resolution:

- the panoramic view (in navigation, the turns before a move, the
  inventory bar's backdrop, the dialog lines and the subject menu over the
  panorama): the same projection as the original's, sampled from the
  panorama (its sprites composited);
- the 3D talking heads, rasterised at that resolution with the
  projection scaled;
- the subtitles (and the options menu's text), drawn from the font's
  outlines at that resolution, smoothed;
- the rest, the original's 640x480 art (the cursor, the inventory bar,
  the guests' portraits, the menus, the puzzles, the dialog videos'
  bodies, the videos), upscaled.

The engine still draws its 640x480 frame unchanged (the scripts, the hit
tests and the tests see the same pixels, whatever the options); for a
frame that shows the panorama it also draws the HD picture and the
640x480 picture that this one stands for, and the front end shows the HD
pixel wherever the frame still shows that picture, the frame's own pixel
(upscaled) where something was drawn over it. The engine also records
the subtitle lines it draws (their places, colours and shadows, and the
picture before and after them): the front end puts back what was under
the 640x480 glyphs (the HD view, or the upscaled image) wherever the
frame still shows them, so that no blocky glyph shows around the sharp
ones, then draws the lines at the window's scale, glyph for glyph where
the original draws them. A cursor drawn over a subtitle stays.

The options (options menu, F10; `settings.ini`):

- Panorama filter, Bicubic (default) or Bilinear. Bicubic upsamples the
  panorama to twice its size with Catmull-Rom (each channel kept between
  its two central texels, so that edges do not ring), which the view then
  samples bilinearly: as sharp as a bicubic view (the view magnifies the
  panorama 1.5 to 6 times) at the cost of a bilinear one.
- Panorama sharpening, On (default) or Off: a mild unsharp mask of the
  panorama (5/8 of the difference from its 3x3 blur), each pixel kept
  within its neighbours' range, so without halos.
- Interface, Smooth (default) or Pixels. Smooth upscales the 640x480 art
  with xBR (after Hyllian's 2xBR, written for the port; twice for 4x, 2x
  up to a 1600-pixel-wide picture), then resamples it bilinearly to the
  window's size; the videos' frames (moves, cinematics), which are
  photographs, are upscaled bilinearly instead (the engine marks them),
  as the dialog videos' bodies are. Pixels shows them as the original
  mode does (nearest).
- Sharp subtitles, On (default) or Off.

The HD size is a multiple of the original's 40x30 grid of 16x16 tiles:
tiles of the window's picture width / 40 pixels, rounded up, at most 64 (4
times 640x480; 32, twice, in the web build). The view is rendered again
only when it changes (the angles, the field of view, the sprites, the
options); the prepared panorama (sharpened, doubled) is computed again
only in the 64x64 tiles a sprite's animation changed; the xBR art only in
the 16x16 blocks that changed and are shown. On desktop the work is split
over the processor's cores; the web build runs it on one. On an 8-thread
laptop processor (`cargo test --release --test hd -- --ignored
--nocapture hd_timings`):

| | 1280x960 | 1920x1440 | 2560x1920 |
|---|---|---|---|
| HD view, bilinear | 1.5 ms | 3.2 ms | 5.4 ms |
| HD view, bicubic (doubled panorama) | 1.7 ms | 3.6 ms | 5.4 ms |
| composition, view and a changing bar | 1.7 ms | 3.5 ms | 5.0 ms |
| composition, full-screen art, still | 1.3 ms | 6.5 ms | 4.5 ms |
| composition, full-screen art, first frame (xBR) | 7 ms | 36 ms | 29 ms |

Preparing a new panorama (sharpened and doubled) takes about 14 ms, once
per place; a sprite's animation frame about 2 ms.
`crates/atlantis-engine/src/hd.rs` holds the HD layer, the options and
the composition; `upscale.rs` the filters (bicubic weights, sharpening,
the panorama's preparation, xBR); the generalised renderer is
`Warp::render_tiles` in `warp.rs` (at 16-pixel tiles it gives the
original's pixels, which the tests check).

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

The web build has been built in CI but little tried in browsers. The window
(title bar, taskbar, Alt+Tab) shows the icon of the player's own
`atlantis.exe`, read at start-up like the executable's tables; the files
we distribute have no icon yet (the game's pictures are not ours to
distribute), so the program file and the macOS app keep the system's
default icon.

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
- F10 opens the port's options menu; the [HD mode](#hd-mode) it offers is
  off by default.

## Licence

GNU General Public License, version 3 or later (see `LICENSE`). The code is
derived from ScummVM's CryOmni3D engine, Copyright (C) the ScummVM team.

The subtitle font, Liberation Sans Regular
(`assets/fonts/LiberationSans-Regular.ttf`, Copyright (c) 2010 Google
Corporation, Copyright (c) 2012 Red Hat, Inc.), is licensed under the SIL
Open Font License, version 1.1 (see `assets/fonts/LICENSE-LiberationSans`).

*Atlantis: The Lost Tales* is a trademark of its owners. This project is not
affiliated with them.
