# Atlantis Native: technical notes

[Back to the README](../README.md)

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
`settings.ini` (see [Players, checkpoints and settings](GUIDE.md#players-checkpoints-and-settings)),
which the first start writes with the defaults; the options menu (F10)
turns them on and off.

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

## Tests

Tests that need the game files read the game directory from the
`ATLANTIS_DATA` environment variable and are skipped without it:

```sh
ATLANTIS_DATA=/path/to/game cargo test --release
```

The expected values in the tests are digests and numbers computed by the
original game's own code; they contain no game data.

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

