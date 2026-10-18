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
| `minor_bug_fixes` | on | the music loops seamlessly (the original's streaming ring has audible seams); the SPF overlay of some talking heads (the guards' coif, the pilot's scarf) is drawn over the head (the original draws it where the next copy erases it); the 3D lower face of some talking heads (Meljanz, Actyon, Servage, the Queen, Gimbas) no longer jumps away from the face on the first frame of each loop of the video (frame 0 of their head animation is a rest pose; it is drawn with frame 1's pose); with the 1997 builds, the hexagon puzzle always shows its pieces, as the later builds do; with the May 1997 French build, LOADMUSIK copies its tracks one by one whatever the music volume, as the other builds do (see [Game builds](#game-builds)) |
| `restored_content` | off | CHAPI063's timer lines of Anna can play (each condition starts from a clear key buffer); with the 2001, English 1997 and May 1997 French builds, a second death in a section plays the next narrator line, as the 1999 and August 1997 French builds do |
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
  outlines at that resolution, smoothed; the 1997 build's pixel font as
  sharp blocks, each of its pixels the size of a 640x480 pixel;
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
- Sharp subtitles, On (default) or Off. With the 1997 build, whose
  subtitles are an 8x8 pixel font, On draws its pixels as sharp blocks
  (no outlines to draw from); Off upscales them with the rest of the
  interface art (xBR when Interface is Smooth).
- Widescreen, Off (default) or On. In a window wider than 4:3, the bars
  beside the picture show the panorama: the view is rendered with more
  tile columns on each side (`Warp::set_side_tiles`), whose grid points
  carry the original's formulas (`FUN_00451915`'s) past the screen's
  edges; the 40 columns of the original's view are its pixels, unchanged
  (a test checks it), so the 4:3 picture, its interface and its hit areas
  stay as they are. The sides show where the frame has an HD layer (the
  panoramic view, the dialog lines over it); the videos, menus and
  puzzles keep black bars. At most 16 tile columns per side (a 2.4:1
  window).
- Video sides (with Widescreen): what fills the bars beside a video's
  frame, which has no picture past its edges. Black (default); Blurred:
  the frame's outer quarter, mirrored at its edge, averaged in 20x20
  blocks, box-blurred and darkened away from the picture; Zoomed: the
  frame enlarged to the window's width (bilinear), its top and bottom
  cut, the subtitles drawn where the 4:3 picture has them; Panorama: for
  a move's video, the side columns of the HD view the turn ended on,
  crossfading over the video to those of the destination's panorama at
  the destination angles (decoded and rendered before the video, without
  its sprites; not known for a move into another area, which keeps the
  view left), with the blurred edges weighing 4t(1 - t) in the middle;
  the cinematics get the blurred edges.

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

The 2001, GOG.com and August 1997 French builds draw their subtitles
with the system's Arial at 13 pixels, not smoothed. The port draws them with Liberation Sans,
which has Arial's metrics, at the same size, rasterised by `ab_glyph` and
thresholded to one bit, so every platform draws the same pixels; the text
bytes are read in the release's code page (`TextCodePage` in
`crates/atlantis-data/src/text.rs`): Windows-1252, but for the May 1997
French CD, whose texts are in the DOS code page 850. The font is bundled
in `assets/fonts`.

The 1997 English CD has no Windows text: it draws its subtitles with the
8x8 pixel font `DIALOG\FONTS.BIN` of the game's archives (one glyph per
byte, the IBM PC set), white over a black shadow, in rows 10 pixels apart
broken in a 75-byte window, and the port does the same (`BitmapFont`,
`split_subtitle_1997`). The original does not clip its rows: the three
shipped rows wider than the screen go on at the left of the next pixel
row, as they do here. The May 1997 French CD draws its subtitles the same
way, ten of its rows wider than the screen; its texts are in the DOS code
page, whose accented letters (`0x82` é, `0x85` à...) are those of the
font's IBM PC glyphs, so the accents show as they should, byte for byte.
Whatever shows these texts otherwise (the outline font, tests) reads
them as code page 850.

### Game builds

The port knows six builds of `atlantis.exe` (`ExeBuild` in
`crates/atlantis-data/src/exe.rs`), found by their size and the PE
header's timestamp (the 2001 build and the patch 1.01 build have the same
size); their tables are read where each build has them, and the code
differences that change what a player sees follow the build:

| | 2001 English CD | GOG.com (1999) | English retail CD (May 1997) | French retail CD (August 1997) | French retail CD (May 1997) |
|---|---|---|---|---|---|
| subtitles | Arial, 14-pixel rows | Arial | 8x8 font, 10-pixel rows | Arial | 8x8 font, 10-pixel rows (DOS code page texts) |
| SUBTITLES item | three-click cycle | three-click cycle | plain toggle | plain toggle | plain toggle |
| cursor ids 0x219 / 0x21a | navigate cursor | talk cursor | talk cursor | talk cursor | talk cursor |
| Rhea always on camera 2 | yes | no | no | no | no |
| second death in a section | same narrator line | next line | same line | next line | same line |
| game-over voice | after the image | after the image | before the image | after the image | before the image |
| players.atl | 97 bytes | 97 bytes | 95 bytes; names in capitals, 0x20..0x7a | 95 bytes | 95 bytes; names in capitals, 0x20..0x7a |
| new-player row | `#` marker, label without it | `#` marker, label without it | the label itself | the label itself | the label itself |
| hexagon puzzle pieces | always | always | with object 402 in hand or FLAGTEMP6 = 2 | with object 402 in hand or FLAGTEMP6 = 2 | with object 402 in hand or FLAGTEMP6 = 2 |
| ogre queen reveal | 3 frames of 1 s | 3 frames of 1 s | 5 frames of 0.375 s | 3 frames of 1 s | 5 frames of 0.375 s |
| rat puzzle pass | 5 ticks after each | 5 ticks after each | a 5-tick deadline | 5 ticks after each | a 5-tick deadline |
| warp cursor animation | two steps a frame | two steps a frame | one step a frame | one step a frame | one step a frame |
| LOAD scrollbar (12+ checkpoints) | knob from the bar's top | knob from the bar's top | knob from y 0, not clamped | knob from y 0, clamped at the bar's top | knob from y 0, not clamped |
| LOADMUSIK / DELMUSIK | as listed | as listed | nothing when the first track plays | as listed | nothing when the first track plays; LOADMUSIK only at a current music volume of 5 or more, and all or nothing on its first track |
| StartMusik of the playing track | fade-in re-armed | fade-in re-armed | fade-in re-armed | fade-in re-armed | nothing |
| credits letters E b d g n t | | | | | one pixel narrower |

Cryo's Windows patch 1.01 (`AT1CDINT.EXE`, December 1999) replaces
`atlantis.exe` (and `cryo.dll`) in a CD installation, English, French or
German, and keeps its `BIGCD1.BIG` … `BIGCD4.BIG`. Its executable is the
1999 build that GOG.com later edited for its release: everything in the
GOG.com column applies to it, and its tables are at the GOG.com build's
addresses. Its interface is French whatever the CD (the new-player row
reads `NOUVEAU JOUEUR`), so the options menu shows *CD, patch 1.01,
French (1999 build)*. GOG.com's own edits of that build (no pause when
the window is inactive, no free-space checks, a delay before each row
copy, no start-up CD scan, an immediate exit on closing) concern how the
original runs on a later system, not the game; the port handles the
window, the disk and the pace in its own way for every build.

In every build the player-name font (FONTMIN) has advances for 101
glyphs, and the original reads those of the glyphs past them from the
credits font's table that follows it; the port does the same with each
build's table (in the August 1997 French build a typed `£` advances 2 pixels,
in the 2001 build 14).

The May 1997 French build is the English 1997 build's code, compiled five
days earlier against an older `cryo.dll`, with its own music copy rules.
By default the port follows them; with the minor bug fixes (on by
default) LOADMUSIK copies its tracks one by one as the other builds do,
whatever the music volume, so that no listed track is left uncopied. The
playing track's StartMusik does nothing in either case. As for every
build, playback reads the archives, so a track the original would not
have copied still plays. For the seven dialog lines whose talking head
has no INITDIAL record, that build's `cryo.dll` reports the failed open
differently and what the original then does is not established; the port
does what it does for the other builds (the error box, the line skipped).

The port's cursor animation steps every 80 ms for the two steps a frame
of the later builds, every 160 ms for the 1997 builds. Where the analysis
of the 1997 builds leaves an effect open, the port keeps the 2001 build's
behaviour: the English one's DelMusik also deletes the playing track's
(open) file, both builds' rat puzzle trajectory search past the end of a
trajectory, a LOAD row past the list. The English one's game-over
narration is started at entry from the current dialog letter, which is
the 2001 build's letter for every game over of the shipped scripts.

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
`ATLANTIS_DATA` environment variable (the 2001 English CD) and are
skipped without it; `ATLANTIS_DATA_1997` names a folder of the 1997
English retail CD, `ATLANTIS_DATA_FR1997` one of the August 1997 French
retail CD, `ATLANTIS_DATA_FR1997_MAY` one of the May 1997 French retail CD
(the first pressing), `ATLANTIS_DATA_PATCH101 (and ATLANTIS_ISO_FR / ATLANTIS_BIN_EN: folders of the French CD's ISO images and the English CD's raw .bin images, for the disc-image reader)` a CD installation with the
patch 1.01 executable, `ATLANTIS_GOG` a folder of the GOG.com languages'
trees (`en-US`, `fr-FR`, ...) and `ATLANTIS_GOG_FR` the French GOG.com
installation, for the tests of those builds:

```sh
ATLANTIS_DATA=/path/to/game ATLANTIS_DATA_1997=/path/to/1997 \
ATLANTIS_DATA_FR1997=/path/to/fr1997 ATLANTIS_DATA_FR1997_MAY=/path/to/fr1997may \
ATLANTIS_DATA_PATCH101=/path/to/patch101 cargo test --release
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

