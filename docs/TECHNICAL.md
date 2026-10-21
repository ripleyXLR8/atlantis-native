# Atlantis Native: technical notes

[Back to the README](../README.md)

## Layout

| Crate | Content |
|---|---|
| `atlantis-data` | BigFile archives, executable tables, game detection |
| `atlantis-codecs` | HNM6 panoramas and videos, APC sound |
| `atlantis-engine` | the game, independent of any platform: input events in, 640x480 frames and audio out |
| `atlantis-app` | the window, input and audio (winit, softbuffer, cpal; in the browser WebGL 2 draws the HD view and composes the HD picture with the engine's integer formulas, bit for bit, and presents it) |

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
| `minor_bug_fixes` | on | the music loops seamlessly (the original's streaming ring has audible seams); the SPF overlay of some talking heads (the guards' coif, the pilot's scarf) is drawn over the head (the original draws it where the next copy erases it); the 3D lower face of some talking heads (Meljanz, Actyon, Servage, the Queen, Gimbas) no longer jumps away from the face on the first frame of each loop of the video (frame 0 of their head animation is a rest pose; it is drawn with frame 1's pose); with the 1997 builds, the hexagon puzzle always shows its pieces, as the later builds do; with the May 1997 French build, LOADMUSIK copies its tracks one by one whatever the music volume, as the other builds do (see [Game builds](#game-builds)); with the German CD, the `ß` of the LOAD list's names (byte 0xe1, which no sprite font has a glyph for: the original draws nothing and moves on 7 pixels) is drawn as two `s` ("verläßt" reads "verlässt"), in the rows and in the centred title, and so in every text drawn with the menu, player-name and credits fonts; with the Dutch CD, the credits text and the player-name fonts are disc 1's newest revision whatever the disc (the original reads them from the disc in the drive, and disc 2 holds an older revision, see [Game builds](#game-builds)); with the Italian CD, a menu line at a fixed place answers over 22 pixels per byte, as in every other build, so the whole drawn word highlights and takes the click (the original's 16 pixels per byte stop short of it: the right end of `OK` and `Annulla`, see [Game builds](#game-builds)); with the Simplified Chinese CD, the subtitle row after an ideographic space starts after the whole space, not on its second byte (see [Subtitles](#subtitles)); with the Chinese CDs, the options' SUBTITLES toggle record is read in step while SUBTITLES is off (the original reads it one byte out of step, so its click area takes the wrong width and height, see [Game builds](#game-builds)) |
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
  window). A dialog's character cut by one edge of the 4:3 picture (the
  original's often are) is moved to the wide picture's edge, with its 3D
  head and its SPF overlay, so that the cut does not show in the middle
  of the screen: its pixels are those the body video's runs draw
  (`ubb_ia_coverage`; some equal the view's), the head's and the
  overlay's, but not those the head's drawing puts back to the view.
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
  A move's video in Zoom mode starts as the 4:3 picture beside the
  place's panorama and zooms in over the first 15% of the video (eased),
  then back out over the last 15% beside the destination's panorama: the
  whole picture is enlarged, the frame from the video, the sides from the
  panorama, so that they keep continuing each other. While the sides show,
  each join of the frame and a side is blurred over 20 pixels (at an
  800-pixel-high picture, in proportion otherwise), the more the nearer
  the join. The sides are
  rendered at the video's own angles (the transition's start angles, and
  the destination's): measured on ATLAN1's moves, a video's first and last
  frames are the panorama at those angles (scale 0.997 to 1.009, no
  offset), while the turn before the video stops a few pixels short of
  them. The video's last frame keeps its sides until the destination's
  view is shown (it stays on screen while the destination loads).
  In every mode the video's sides reach no further than the panorama's
  (16 tile columns, `hd::MAX_SIDE_TILES`): in a window wider than 2.4:1
  the zoomed frame is the 2.4:1 width and the rest stays black, so a
  video keeps the frame of the views around it.

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

The 2001, GOG.com, August 1997 French and German, and the Spanish and
Italian builds draw their subtitles with the system's Arial at 13 pixels, not smoothed. The port draws them with Liberation Sans,
which has Arial's metrics, at the same size, rasterised by `ab_glyph` and
thresholded to one bit, so every platform draws the same pixels; the text
bytes are read in the release's code page (`TextCodePage` in
`crates/atlantis-data/src/text.rs`): Windows-1252, but for the May 1997
French CD and the Dutch and Swedish CDs, whose texts are in the DOS code
page 850.
The font is bundled
in `assets/fonts`.

The Chinese CDs ask Windows for a face named `MingLi` with a 16-pixel
cell, which no Windows has: a Chinese Windows draws their subtitles with
its MingLiU (Traditional) or SimSun (Simplified), the bytes read in its
code page (Big5, GBK), in rows 18 pixels apart over the black shadow.
The port draws them with the player's own `mingliu.ttc` / `simsun.ttc`
when one is beside the game's files or the discs' images (the face of
that name in the collection), else with Noto Serif TC / SC, the same
Ming / Song style, bundled as subsets to the characters of the two
releases' scripts plus ASCII and the punctuation
(`assets/fonts/make_cjk_subset.py` reads the characters from the discs),
at 16 pixels per em, monochrome, by the TrueType scan conversion rules
(a pixel whose centre is inside the outline or on it is on; where a row
or a column of pixel centres crosses a stroke between two centres and
neither pixel is on, the one nearer the stroke is on: the dropout control
that keeps the thin horizontals of a Ming / Song face at that size). A
two-byte character the face has no glyph for (the code page's
user-defined area) draws nothing but keeps its full-width cell. The size
Windows renders the face at, and whether it drew its embedded 16-pixel
bitmaps, are not known. MingLiU before version 7.00 (Windows XP's and
older) draws right only through its hinting, which the port does not
run, and is not used. In the HD mode the sharp subtitles use the same
face at the output's resolution.

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
So does the Dutch CD (`é ë è ï ú`), eight of its rows wider than the
screen. The Swedish CD (`ä å ö Ä Å Ö é`) breaks its rows another way: always
at the last space before the end of the 75-byte window, never at a space
after it, so none of its rows is over 75 bytes or wider than the screen
(`split_subtitle_1997_back`). A run of 75 bytes without a space, which no
shipped text has, makes that original fault; the port cuts it at 75
bytes.
Whatever shows these texts otherwise (the outline font, tests) reads
them as code page 850.

The Chinese CDs break their rows in a 75-byte window too, but cut after a
double-byte punctuation mark, found backwards from the window's end
without following the lead bytes (`subtitles/cjk.rs`): the Traditional
build after Big5 `，。；？！…`, the Simplified one after GB2312 `，。？！`
or before an ideographic space `　`, the next row then starting on the
space's second byte, so that row's characters are read one byte out of
step (72 rows of the shipped texts). The port does the same, row for row
(the tests compare every dialog line with the original functions run
under emulation); with the minor bug fixes, the row after an ideographic
space starts after the whole space (27 of those rows mended; the others
come from a mark the scan reads across two characters, `。　` read as
`！`). The original's arithmetic is kept as it is: after the first row
the checks read one byte past the mark, and a later window without a
mark gives empty rows. A first window without a mark sends the
original's scan back past the start of its buffer (one Traditional
line, CHAPI084's game-over narration, 80 bytes): what it then does is
unknown, and the port cuts that row at the last character boundary
within 75 bytes. The buffer holds the line as the dialog code copies it,
up to the CR, so the blanks that end most Traditional lines (and some
Simplified ones) count: with its final space, a text of 75 bytes is two
rows, the second mostly the space alone (17 Traditional lines, 2
Simplified ones). The other builds' breakers are given the text without
them.

### Game builds

The port knows thirteen builds of `atlantis.exe` (`ExeBuild` in
`crates/atlantis-data/src/exe.rs`), found by their size and the PE
header's timestamp (the 2001 build and the patch 1.01 build have the same
size, as have the August 1997 French and German builds and the Spanish
and Italian ones, and the English, Dutch and Swedish 1997 retail
builds); their tables are read where each build has them, and the code
differences that change what a player sees follow the build:

| | 2001 English CD | GOG.com (1999) | English retail CD (May 1997) | French, German, Spanish and Italian retail CDs (1997) | French retail CD (May 1997) | Traditional and Simplified Chinese CDs (1998, 1999) |
|---|---|---|---|---|---|---|
| subtitles | Arial, 14-pixel rows | Arial | 8x8 font, 10-pixel rows | Arial | 8x8 font, 10-pixel rows (DOS code page texts) | MingLiU / SimSun, 18-pixel rows (Big5 / GBK texts) |
| SUBTITLES item | three-click cycle | three-click cycle | plain toggle | plain toggle | plain toggle | three-click cycle, on at start-up |
| cursor ids 0x219 / 0x21a | navigate cursor | talk cursor | talk cursor | talk cursor | talk cursor | talk cursor |
| Rhea always on camera 2 | yes | no | no | no | no | no |
| second death in a section | same narrator line | next line | same line | next line | same line | next line |
| game-over voice | after the image | after the image | before the image | after the image | before the image | after the image |
| players.atl | 97 bytes | 97 bytes | 95 bytes; names in capitals, 0x20..0x7a | 95 bytes | 95 bytes; names in capitals, 0x20..0x7a | 114 bytes |
| new-player row | `#` marker, label without it | `#` marker, label without it | the label itself | the label itself | the label itself | the label itself |
| hexagon puzzle pieces | always | always | with object 402 in hand or FLAGTEMP6 = 2 | with object 402 in hand or FLAGTEMP6 = 2 | with object 402 in hand or FLAGTEMP6 = 2 | with object 402 in hand or FLAGTEMP6 = 2 |
| ogre queen reveal | 3 frames of 1 s | 3 frames of 1 s | 5 frames of 0.375 s | 3 frames of 1 s | 5 frames of 0.375 s | 3 frames of 1 s |
| rat puzzle pass | 5 ticks after each | 5 ticks after each | a 5-tick deadline | 5 ticks after each | a 5-tick deadline | 5 ticks after each |
| warp cursor animation | two steps a frame | two steps a frame | one step a frame | one step a frame | one step a frame | one step a frame |
| LOAD scrollbar (12+ checkpoints) | knob from the bar's top | knob from the bar's top | knob from y 0, not clamped | knob from y 0, clamped at the bar's top | knob from y 0, not clamped | knob from the bar's top |
| LOADMUSIK / DELMUSIK | as listed | as listed | nothing when the first track plays | as listed | nothing when the first track plays; LOADMUSIK only at a current music volume of 5 or more, and all or nothing on its first track | as listed |
| StartMusik of the playing track | fade-in re-armed | fade-in re-armed | fade-in re-armed | fade-in re-armed | nothing | fade-in re-armed |
| credits letters E b d g n t | | | | | one pixel narrower | |

The German retail CD's build (August 1997) is the August 1997 French
build's code byte for byte, built again with German strings: everything
in that column applies to it. Its new-player row reads `NEUER SPIELER`,
and as that string is one byte shorter, its credits font table (which
the player-name font's glyphs past 101 read, see below) starts one byte
earlier. Its CON texts are Windows-1252; its menu, LOAD-list and credits
texts hold the umlauts at the DOS code page's places, where the sprite
fonts have their glyphs. Those fonts have no `ß`: the three LOAD-list
names that spell it (`verläßt` twice, `Außerhalb`) show a gap of 7
pixels in its place in the original, and two `s` in the port with the
minor bug fixes (see [Enhancements](#enhancements)).

The Spanish retail CD's build (August 1997) is the August 1997 French
build's code but for one line, the reference's: the rat puzzle's search
for the rat's place on its trajectory (T0023) starts again from the
trajectory's first point, where the French build searches on from where
it was. Its new-player row reads `Jugador nuevo`, one byte shorter than
the French one, so its tables are at the German build's addresses; its
credits font table holds the 2001 build's bytes (the typed `í ú ñ` of
the LOAD list advance 10, 14, 14 pixels; 17, 2, 10 in the French
build).

The Italian retail CD's build (July 1997, eleven days before the French
one) is the August 1997 French build's code but for the menu walk: a
menu line at a fixed place (`<x,y>`) answers the pointer over 16 pixels
per byte of its text instead of 22, and a menu byte above 0xa8 is drawn
as a space (no Italian menu has one). The zones fall short of the drawn
words (the zone of `OK` ends 11 pixels before the end of its `K`, that
of `Annulla` 18 pixels before its end), so the right end of those
buttons neither highlights nor takes a click; the port does the
same, and with the minor bug fixes gives them the 22 pixels of every
other build (see [Enhancements](#enhancements)). Its new-player row
reads `Nuovo Giocatore`, which moves its credits font table one byte up
from the French build's.

Both have Windows-1252 CON texts (the Spanish `¡ ¿ á é í ñ ó ú ü`..., the
Italian `à è é ì ò ù`), broken into rows as the 2001 build breaks them;
their menu, LOAD-list and credits texts hold their letters at the DOS
code page's places, where the sprite fonts have a glyph for each.

The Dutch retail CD's build (June 1997) is the English May 1997 build's
code: everything in that column applies to it, but for the names typed
on the player screen, taken from 0x20 to 0xa7 and kept as typed (as the
later builds; not in capitals). Its new-player row reads `Nieuwe
speler`, which moves its credits font table 3 bytes and its inventory
and guest tables 4 bytes up from the English build's. Its start-up
plays the R&P Electronic Media logo (`UBB_VUE\LOGORP.HNM`, 60 frames,
silent) after the Cryo logo and its one-second pause, before PREINTRO,
at the cinema pace. Its texts are in the DOS code page.

The Swedish retail CD's build (June 1997) is the English May 1997
build's code too, with every table at the English build's address: its
new-player row reads `Ny spelare`. It has the Dutch build's name entry
(0x20 to 0xa7, kept as typed) but not its R&P logo, and three changes
of its own: the subtitle rows are broken at the last space of the
75-byte window (see [Subtitles](#subtitles)); the name entry also takes
`Ä Å Ö ä å ö` and stores them as the DOS code page bytes the player-name
font draws (0x8e 0x8f 0x99 0x84 0x86 0x94); the LOAD scrollbar's knob
starts at y 0 but is clamped at the bar's top, and a row click counts
the list's offset, as in the later builds. Its texts are in the DOS code
page.

The Traditional Chinese 4-CD release's build (July 1998) and the
Simplified Chinese one's (March 1999) are the French August 1997 build's
code moved toward the 2001 build by an intermediate 1998 build (the
three-click SUBTITLES item, the LOAD scrollbar's knob and clamp, the rat
puzzle's restarted search, the 2001 credits widths), with changes of
their own; the Simplified build is the Traditional one with another
subtitle breaker. Their tables are at the same addresses (the
new-player row `New Player`, no `#`), and their texts are double-byte:
Big5 (code page 950) and GBK (code page 936), decoded with `encoding_rs`.
Their discs are detected, whether copied or as images (the Simplified
discs name their files in lower case), and the game runs. Their own
rules (`ExeBuild::cjk_breaker`, `subtitle_row_step`,
`menu_sprite_banks`, `menu_toggle_sprites`, `load_list`,
`cinema_captions`, `subtitles_on_at_start`, `players_names_block`,
`off1_player`) are fields of their build entries; their subtitles are
described in [Subtitles](#subtitles). Their discs are the ones ScummVM
detects by the md5 of the first 5000 bytes of `atlantis.exe` and
`BIGCD1.BIG` and the archive's size (a disc test checks it); the port,
as for every build, tells them apart by the executable's size and PE
timestamp.

Their interface is drawn as theirs: the menus are sprite records of
SPRCHI1.SPR (SPRCHI2.SPR on the hovered record, labels included), the
options toggles are sprite records too (while SUBTITLES is off the
original reads its record one byte out of step: the record keeps the
place an earlier walk gave it, with a wrong width and height; the minor
bug fixes read it in step);
the LOAD list shows EPICHI1.SPR frame *n* on row *n* (EPICHI2.SPR when
hovered) whatever checkpoint the row holds, and a row loads the chapter
of the executable's table at its position, its title centred by the
table's width, and the scrollbar's knob is drawn at the place the
picker gives its record (the original reloads a sprite record's stored
y, in every build); the opening cinematic (`CINEMA=20`) has its
captions from SUBCHINE.SPR on the 200 Hz ticks of the original, over
that video only (the original keeps the CINEMA number 20 until the next
CINEMA command and frees the bank after the video, so another video
played within 94 s of the opening's start, when it is cut short, would
be drawn over from the freed bank: what shows is not known); SUBTITLES is
on at every start-up; `players.atl` is 114 bytes (a 102-byte names
block, a deletion with four players keeping the New Player row past the
count, as the original does); an `off1` dialog line (29 in each
release's scripts) is played by the off1 player: off0's, with the screen
buffer (the first IX buffer, what the last video or LOADMUSIK copy left)
copied to the screen once the voice has loaded. The GOG.com executable
has that player too, for its scripts' 30 `off1` lines, but the port keeps
playing those as `off` lines, the view redrawn, as before (a
difference).

The original opens one archive at a time, the inserted disc's. What it
loads at start-up (the menus, the CD and error texts, EPI.TXT, the menu
font, PREINTRO.TGA, and in the 2001, 1999 and August 1997 builds
CREDITS.TXT) comes from the disc it starts from, disc 1 in the port;
what it reads when it uses it comes from the disc in the drive: the
player-name fonts FONTMIN / FONTMIN2 (on entering the LOAD list and the
player screen) and, in the 1997 builds of the English May code (English,
French May, Dutch, Swedish), CREDITS.TXT. The archive keeps the copies of each
disc (`BigArchive::set_disc_copy`), and the port reads these members
from the current disc's archive. Only the Dutch CD has copies of these
that differ (the Swedish discs 2 to 4 hold older CD texts, EPI.TXT and
SELEMENU.TXT, which the original never shows, and the same CREDITS.TXT
and FONTMIN fonts as disc 1): its disc 2 holds an older revision (the menus at fixed places,
EPI.TXT's typos, `Musiek`, CREDITS.TXT one line shorter,
PREINTRO.TGA without the R&P and Cryo logos, FONTMIN fonts with other
`j` and `'` hotspots). So
while disc 2 is in use, its CREDITS.TXT and FONTMIN show, as in the
original; with the minor bug fixes, disc 1's, the newest, whatever the
disc.

Cryo's Windows patch 1.01 (`AT1CDINT.EXE`, December 1999) replaces
`atlantis.exe` (and `cryo.dll`) in a CD installation, English, French,
German, Dutch, Swedish, Spanish or Italian, and keeps its `BIGCD1.BIG` … `BIGCD4.BIG`. Its executable is the
1999 build that GOG.com later edited for its release: everything in the
GOG.com column applies to it, and its tables are at the GOG.com build's
addresses. Its interface is French whatever the CD (the new-player row
reads `NOUVEAU JOUEUR`), so the options menu shows *CD, patch 1.01,
French (1999 build)*. Over the Dutch or Swedish CD, whose texts are in
the DOS code page, it draws them as Windows-1252, as it passes them to
Windows: the accented letters show as other signs (the Swedish `ä` as
`„`), and its name entry has no Swedish letters. GOG.com's own edits of that build (no pause when
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

### The desktop front end and the game library

`crates/atlantis-app/src/main.rs` decides what the window starts with:
the game folder of the command line, else the game library
(`library.rs`, the file `games.txt`) through `session::startup`: its one
game, the first-start flow of `desktop.rs` (detection, explanations, the
folder picker, the copy from the discs) when it is empty, or the
library's screen (`launcher.rs`, the state, input and drawing on the
640x480 screen; `session.rs` carries out its actions). The `App` can
drop its engine and start another without restarting the process:
`App::stop_game` drops the `Runner` (the engine's async task: the
engine runs on the event-loop thread and its worker threads are scoped
to each frame, so nothing is left running) and closes the sound stream,
frees the pointer and closes the menu; `App::install_game` takes the new
`Sys` and `Runner`, reopens the sound stream on the new mixer, and puts
the new release's title, icon and settings store (one per build) in
place. A test checks that the old `Sys` is freed. The web build keeps
its single folder picked in the page.

### The web build

`web/build.sh` builds it (`atlantis-native-web.zip` on the nightly
release holds the result); serve the folder over HTTP (for example
`python3 -m http.server -d web 8000`), the page asks for the game folder. The engine
reads the player's files through an asynchronous loader that fetches each
archive member before it is read, so the browser runs the same game as the
desktop. A browser grants
the pointer capture only on a click: until then the view turns with the
pointer's own motion. Sound starts on the first click.

In the browser the graphics card does the HD mode's heavy work, with the
engine's own integer arithmetic so that the picture is the same, pixel
for pixel:

- The HD view: a fragment shader runs `Warp::render_tiles`'s tile
  interpolation and the bilinear samplers on an integer copy of the
  prepared panorama, sent whole for a new panorama and by its changed
  tiles otherwise (`PanoramaPrep::take_changes`). Where the engine needs
  the view's pixels (a dialog's head is composed over it, the widescreen
  video sides) it draws them on the processor on demand.
- The composition: `HdCompositor::gpu_plan` prepares a frame as `compose`
  does (the text uncovered, the scale maps, the xBR art brought up to date
  and its changed blocks); a shader runs `compose`'s loop per output pixel
  with the maps sent as integer tables, the layer being the card's view or
  the engine's pixels. The sharp text is still rasterised by the
  processor, into a premultiplied overlay over the rows it uses. A video's
  widescreen sides are composed by the processor.
- The presentation: the composed frame, at most 1280x960 pixels (no
  taller than the HD view), is enlarged to the canvas by the card.

`localStorage["atlantis-native/gpu-view"]`: `off` keeps all of it on the
processor; `check` reads the card's view and composition back and logs
how many pixels differ from the processor's (0 in every test so far).

Comments cite addresses in the original `atlantis.exe` (`FUN_0042844c`,
`0x00496570`, ...) so that each behaviour can be traced to the original code.

## Tests

Tests that need the game files read the game directory from the
`ATLANTIS_DATA` environment variable (the 2001 English CD) and are
skipped without it; `ATLANTIS_DATA_1997` names a folder of the 1997
English retail CD, `ATLANTIS_DATA_FR1997` one of the August 1997 French
retail CD, `ATLANTIS_DATA_FR1997_MAY` one of the May 1997 French retail CD
(the first pressing), `ATLANTIS_DATA_DE1997` one of the German retail CD,
`ATLANTIS_DATA_NL1997` one of the Dutch retail CD,
`ATLANTIS_DATA_SV1997` one of the Swedish retail CD,
`ATLANTIS_DATA_ES1997` / `ATLANTIS_DATA_IT1997` ones of the Spanish /
Italian retail CD,
`ATLANTIS_DATA_PATCH101` a CD installation with the patch 1.01
executable, `ATLANTIS_DATA_PATCH101_DE` the patch's `atlantis.exe` beside
the German CD's archives, `ATLANTIS_DATA_PATCH101_NL` /
`ATLANTIS_DATA_PATCH101_SV` / `ATLANTIS_DATA_PATCH101_ES` /
`ATLANTIS_DATA_PATCH101_IT` the same beside the Dutch / Swedish / Spanish
/ Italian CD's,
`ATLANTIS_ISO_FR` / `ATLANTIS_ISO_DE` / `ATLANTIS_ISO_SV` /
`ATLANTIS_BIN_EN` / `ATLANTIS_BIN_NL` folders of the French, German and
Swedish CDs' ISO images and the English and Dutch CDs' raw .bin images,
`ATLANTIS_MDF_ES` / `ATLANTIS_NRG_IT` folders of the Spanish CD's .mdf and
the Italian CD's Nero .nrg images (for the disc-image reader),
`ATLANTIS_ISO_ZH_TW` / `ATLANTIS_BIN_ZH_CN` folders of the Traditional
Chinese CD's ISO images and the Simplified Chinese CD's raw .bin images,
`ATLANTIS_ISO_UNKNOWN` a folder of images of an edition not supported
(the report test, skipped when the edition opens), `ATLANTIS_GOG` a folder of the GOG.com languages'
trees (`en-US`, `fr-FR`, ...) and `ATLANTIS_GOG_FR` the French GOG.com
installation, for the tests of those builds:

```sh
ATLANTIS_DATA=/path/to/game ATLANTIS_DATA_1997=/path/to/1997 \
ATLANTIS_DATA_FR1997=/path/to/fr1997 ATLANTIS_DATA_FR1997_MAY=/path/to/fr1997may \
ATLANTIS_DATA_DE1997=/path/to/de1997 ATLANTIS_DATA_NL1997=/path/to/nl1997 \
ATLANTIS_DATA_SV1997=/path/to/sv1997 ATLANTIS_DATA_ES1997=/path/to/es1997 \
ATLANTIS_DATA_IT1997=/path/to/it1997 ATLANTIS_DATA_PATCH101=/path/to/patch101 \
cargo test --release
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

