# Atlantis Native: technical notes

[Back to the README](../README.md)

## Layout

| Crate | Content |
|---|---|
| `atlantis-data` | BigFile archives, executable tables, game detection |
| `atlantis-codecs` | HNM6 panoramas and videos (with Atlantis II's alpha masks), APC sound; shared by both games |
| `atlantis-engine` | the game, independent of any platform: input events in, 640x480 frames and audio out |
| `atlantis2-data` | Atlantis II: detection, the install folder and the four discs, `WAM V2.8` places, `defnames.con`, the place sprites (SPW) and the sprite banks (`SPRCRYO`), the `.db` 3D scenes and head maps, the dialog scripts, the object names of `Messages.txt` |
| `atlantis2-engine` | Atlantis II: the cube panoramas and their camera, the place animations, the cursor and the moves between views (Tibet), the dialogs (lines, voices, body videos, 3D heads, lip-sync, subtitles, music ducking), the CON script interpreter, the menus and the saves, the bag bar, the object in hand and the special objects' overlays, the Tibet mandala puzzle |
| `atlantis-app` | the window, input and audio (it picks the engine by the game the folder holds) (winit, softbuffer, cpal; in the browser WebGL 2 draws the HD view and composes the HD picture with the engine's integer formulas, bit for bit, and presents it) |

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
| `minor_bug_fixes` | on | the music loops seamlessly (the original's streaming ring has audible seams); the SPF overlay of some talking heads (the guards' coif, the pilot's scarf) is drawn over the head (the original draws it where the next copy erases it); the 3D lower face of some talking heads (Meljanz, Actyon, Servage, the Queen, Gimbas) no longer jumps away from the face on the first frame of each loop of the video (frame 0 of their head animation is a rest pose; it is drawn with frame 1's pose); with the 1997 builds, the hexagon puzzle always shows its pieces, as the later builds do; with the May 1997 French build, LOADMUSIK copies its tracks one by one, as the other builds do, but still nothing below a current music volume of 5 (see [Game builds](#game-builds)); with the German CD, the `ß` of the LOAD list's names (byte 0xe1, which no sprite font has a glyph for: the original draws nothing and moves on 7 pixels) is drawn as two `s` ("verläßt" reads "verlässt"), in the rows and in the centred title, and so in every text drawn with the menu, player-name and credits fonts; with the Dutch CD, the credits text and the player-name fonts are disc 1's newest revision whatever the disc (the original reads them from the disc in the drive, and disc 2 holds an older revision, see [Game builds](#game-builds)); with the Italian CD, a menu line at a fixed place answers over 22 pixels per byte, as in every other build, so the whole drawn word highlights and takes the click (the original's 16 pixels per byte stop short of it: the right end of `OK` and `Annulla`, see [Game builds](#game-builds)); with the Simplified Chinese CD, the subtitle rows start and end between characters: the row after an ideographic space starts after the whole space, not on its second byte, and a punctuation mark is only taken where a character starts, not across two characters (see [Subtitles](#subtitles)); with the Chinese CDs, the options' SUBTITLES toggle record is read in step while SUBTITLES is off (the original reads it one byte out of step, so its click area takes the wrong width and height, see [Game builds](#game-builds)); each menu keeps its own click records, from zero at every visit (the original keeps one record table for the whole session, with 16 records per menu in the May 1997 English and French builds and 20 in the others: in those two builds the sound menu's last two records spill over the ARE YOU SURE ? menu's and move its left ornament off screen for the session, and in every build the LOAD scrollbar's knob, hidden by a visit with 11 checkpoints or fewer, stays hidden on the later visits); with five players, OK on the fifth starts it with its name (the original runs the commit of a typed name, empty, over the fifth name: that player stays listed with no name and cannot be chosen) |
| `restored_content` | off | CHAPI063's timer lines of Anna can play (each condition starts from a clear key buffer); with the 2001, English 1997 and May 1997 French builds, a second death in a section plays the next narrator line, as the 1999 and August 1997 French builds do |
| `ui_ux` | off | the error box of the seven lines whose talking head the original cannot open is not shown (the line is skipped either way) |

Atlantis II has its own `Enhancements` (`crates/atlantis2-engine/src/enhancements.rs`),
for now `minor_bug_fixes` alone, on by default: every music track is heard
to its end, the next one of the `startmusik` list starting right after
its last sample (the original drops the last ~5.8 s of a track followed
by another), and a track that ends with an empty list, or with loops
left, plays again from its start (the original loops the last 5.9 s of
its ring until another music command); see the Music item of
[Atlantis II](#atlantis-ii-in-progress). `--original` turns it off there
too.

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
`split_subtitle_1997`). The original does not clip its rows: the rows
wider than the screen (two of the shipped texts', each shown as the first
line after start-up) go on at the left of the next pixel row, as they do
here. The text is broken in a 1 KB buffer that only the dialog lines and
the game-over narration write, each a NUL after its text and the rest
left as it was, and cleared once at start-up: when a text's last word
crosses the window's end the breaker reads on past the NUL, into zeros or
what an earlier, longer line left there, so a line's rows can depend on
the lines shown before it (18 English lines read in file order). The port
keeps that buffer as the original does (`SubtitleTextBuffer`, written for
every line the scripts run, shown or not). The May 1997 French CD draws
its subtitles the same way, eight of its rows wider than the screen; its
texts are in the DOS code
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
under emulation); with the minor bug fixes, the Simplified build's row
after an ideographic space starts after the whole space (27 of those
rows), and its marks are only read where a character starts, so that no
mark is read across two characters (`。　` read as `！`: the other 45
rows); then every row starts and ends between two characters (the tests
compare every line with the original function run with those two
changes). The original's arithmetic is kept as it is: after the first
row the checks read one byte past the mark (with the minor bug fixes, in
the Simplified build, only when that byte starts a character), and a
later window without a mark gives empty rows (with the minor bug fixes,
in the Simplified build, the port's cut below). A first window without a mark sends the
original's scan back past the start of its buffer (one Traditional
line, CHAPI084's game-over narration, 80 bytes): what it then does is
unknown, and the port cuts that row at the last character boundary
within 75 bytes. The buffer holds the line as the dialog code copies it,
up to the CR, so the blanks that end most Traditional lines (and some
Simplified ones) count: with its final space, a text of 75 bytes is two
rows, the second mostly the space alone (17 Traditional lines, 2
Simplified ones). So do the 1997 builds' breakers (the buffer above);
the 2001 build's is given the text without them.

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
| `off1` dialog lines | off1 player (none in the scripts) | off1 player (30 lines) | no such player or line | no such player or line | no such player or line | off1 player (29 lines) |

Two rules that the port once kept for the Chinese builds alone now follow
the original in every build: a menu's sprite record is drawn at the y the
record holds (the LOAD scrollbar's knob at the place the picker gives it,
not the menu file's), and an `off1` line is played by the off1 player
wherever the executable has it (the GOG.com scripts' 30 such lines,
which the port played as `off` lines, the view redrawn, before).

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
y, as every build does); the opening cinematic (`CINEMA=20`) has its
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
copied to the screen once the voice has loaded. The 2001 code line has
that player too (the GOG.com scripts' 30 `off1` lines), the 1997
executables not (`off1_player`).

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
so that a list whose first track is there leaves no other listed track
uncopied; it still copies nothing below a current music volume of 5. The
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
desktop. The Chinese subtitle fonts (about 0.5 MB each) are not built
into the web program as they are into the desktop one: `web/build.sh`
copies them into `web/fonts`, and the page fetches the one a Chinese
release needs once the release is known, before the game starts, unless
the picked folder holds the player's own `mingliu.ttc` or `simsun.ttc`
(its first bytes those of a font), which the engine draws with; the
other releases never load them (the program is 2.7 MB instead of
3.7 MB). A browser grants
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

## Atlantis II (in progress)

A folder holding `atlantis2.exe` opens Atlantis II (`crates/atlantis-app/src/a2.rs`):
the player menu comes first, as in the original (five symbols: a symbol
continues that player's newest save, or starts a new game). As the
original's start-up (@ 0x0049d680, last two calls), the player page is
built first and the intro videos then take the screen over it
(`Engine::start_intro`, @ 0x0049d9c0): `intro%02d.hnm` from 00, each with
its `intro%02d.apc` on voice 2 at the effects volume, the next one started
when one ends (@ 0x0040c0ee), until a file the current disc does not have;
then the video loop gives the screen back to the loop type it started
from (@ 0x00616890), the menu, whose frame draws the player page. So disc
1 plays two videos and the other discs one (the same as disc 1's second);
no music plays, no cursor is drawn. A left click ends the current video
at the next tick and the next one starts; a double click ends them all,
since the video end clears the click flag but not the double click's
(@ 0x0040c0c7); Escape does nothing in the video loop. They play at every
start: nothing records that they were seen. `--no-intro` skips them;
`--intro` plays them in a headless run (which otherwise starts a new game
at once) or with `--menu player`. A new game
runs the chapter scripts as the original's start-up and menu do:
chapter 0's `/INIT` sets the world, the
place and Tibet's first view (`tibet.wam` view 2 at yaw −90°, pitch 0),
plays the opening film with its sound track and then starts the music;
the mouse or the arrow keys turn the view within the script's limits
(pitch ±70° by default, yaw free), the place animations play, the cursor
at the screen centre tells what is under it, a left click on a move zone
walks to the next view as the original does (the turn, the transition
video, the arrival), and the scans of the warp frame run the scripts: the
first click sends the game to chapter 75, the first arrival in view 7
plays the shaman's greeting (dialog 360) and the section goes on after
it. A left click ends a film and goes to a dialog's next line; Escape ends
a dialog, and in the panorama opens the main menu (options, volumes,
load and save, quit; below); the right button opens and closes the bag
bar; a left click on a character with subjects opens the menu of the
subjects to ask him about (below). The mandalas are laid in the shaman's
hut (below). Tibet is the only world: a trip to another one (a boat
laid, the crystal, the space travel) plays its film and stays in Tibet.
Not yet: the other worlds and their puzzles.
`--headless <folder> [frames] --screenshot
<file.png>` plays a new game for that many frames of 1/60 s and saves the
screen;
`--view <id>[,<yaw>,<pitch>]` starts in another view, `--dialog <n>`
plays a dialog at once and `--menu <page>` shows a menu page (`player`,
`main`, `options`, `sound`, `load`, `credits`; testing aids, which end the
opening film first),
e.g. `--view 7,-164.394,14.96 --dialog 360`; `--take <object>[,...]` puts
objects in the bag and `--hold <object>` one in the hand (by their
`defnames.con` names, through the scripts' `sacados` and `objetenmain`),
`--bag` opens the bar, `--pointer <x>,<y>` places the mouse pointer and
`--talk` clicks the character under the centre (e.g. `--view 7,70,5
--talk --pointer 475,329`: the shaman's menu, the pointer on a subject). `Engine::new_game` keeps the
start without the scripts (the view `/INIT` names) for the tests against
the original's moves. The example `cargo run --release -p atlantis2-engine
--example tibet_tour <install> <discs> <out>` writes pictures of three
views with their animations and a recorded walk from the first view to
view 4; `--example menu_png <data folder> <prefix>` draws the menu pages
after a short game with three saves.

- **Build:** `atlantis2.exe` of the French CD (1999, 2199552 bytes, PE
  timestamp 0x38030926), recognised by its size and timestamp and
  confirmed by its MD5 (`crates/atlantis2-data/src/exe.rs`).
- **Files:** the install folder holds the places, scripts, texts and
  sprite banks; everything under `datas_a2\` comes from the current CD.
  The discs (images or copies) are found in the game folder, in the
  folders directly inside it and in each `--discs <folder>`, and told apart
  by the original's marker files (`datas_a2\spf\sphex.spr`, `cerceris`,
  `bousier`, `mante`). The current disc follows the `MONDE` rule (worlds
  1..4 on discs 1..4, Tibet on whichever disc is in, 6 on disc 4); six
  files differ between the discs, so a file is read from the current disc
  first (`crates/atlantis2-data/src/files.rs`). The discs' long names are
  in their Joliet trees, which the disc-image reader now reads.
- **Places:** `WAM V2.8` is a stream of tagged chunks (views, moves, 3D
  click zones with their corners, sounds, links, entities and their
  states), read in file order; the zone under the screen centre is the
  first whose edges all hold the centre's direction
  (`crates/atlantis2-data/src/wam.rs`).
- **Panorama:** a cube of six 512x512 faces (the HNM's six frames, `bk dn
  ft lf rt up`), drawn every frame as six textured quads with a 76° field
  of view and a near plane at z = 2 (`crates/atlantis2-engine/src/warp.rs`).
  The original picks its rasteriser by CPU (C, P5, MMX, PII, K6-3D), and
  they differ by a texel here and there. The port draws with a
  transcription of the Pentium II one (`warpPII`, `warp/pii.rs`), which
  every Intel CPU since the Pentium II used: hand-written x87 and MMX code,
  a fan of triangles ordered by their integer rows, perspective every 32
  pixels and at the span's last pixel, 1.31 fixed point texel coordinates
  stepped in 16-bit words in between (so the texel wraps within the face),
  nearest texel; its span buffer holds rows of up to 4320 pixels, the
  widest screen it draws. The C one (`warpC`: perspective every 32 pixels, 24.8
  fixed point in between), which AMD CPUs used, is transcribed too and
  kept for a later option.
- **Camera:** the orientation is a float quaternion turned by axis-angle
  rotations built through 3x3 matrices, transcribed operation by
  operation under the x87 control word (`fpu.rs`, `camera.rs`): the matrix
  is the original's to the last bit, residues included. `warpPII` loads
  the control word 0x0e7f (53-bit precision, rounding toward zero) and
  never restores it, so after its first frame the camera's arithmetic
  rounds toward zero too; the port models every operation in that mode
  (the exact error of each sum, product, quotient and square root tells
  which way the double was rounded). Before it, the process's word
  applies: 53 bits to nearest (0x027f, Win32's default, the port's
  choice) or 24 bits (0x007f, which Direct3D may have set; not known). A
  new game's first view is entered under the process's word, as in the
  original when no assembly frame was drawn before it (also not known).
- **Turning:** the mouse adds dx / 2000 and −dy / 2000 radians to two
  speeds, which decay × 0.8 a frame; the limits stop the turn (`look.rs`).
  The original's warp loop is unpaced; the port turns once per frame at
  60 frames a second. The arrow keys stand for 8 mickeys a frame each (a
  port convenience).
- **Time:** as in the original, the frame dispatcher runs once per
  drawn frame (`Engine::pass`: the music's state machine, then the loop
  of the moment: panorama, video or dialog) and a 15 Hz task
  (`Engine::tick`, @ 0x0040caa0) paces the videos, the place animations,
  the cursor, the music fades, the dialog's lines and the script clocks;
  `Engine::set_clock` runs the ticks due at the scheduler's clock (a
  stall of over a second drops the ticks missed). The window draws at 60
  frames a second, the headless mode runs frames of 1/60 s. A move's turn is counted in frames, as in
  the original (`engine.rs`).
- **Place animations:** the SPW files of `datas_a2\spw\` are banks of
  frames with up to six sprites each, one per cube face, in the first
  game's sprite rows and blend (`c + dst × (32 − f) / 32`), drawn into the
  512x512 faces before the cube (`crates/atlantis2-data/src/spw.rs`). At
  each view entry every state of every entity shown in that view gets a
  slot (at most 34): first frame, end, click kind, shown or not; the
  show / hide table (a taken object) overrides "shown". Each tick moves
  every shown slot one frame on; at its end a slot restarts (its first
  play one frame shorter, as in the original), stops, or, for a state
  with a weight, draws among the weighted states of its entity with the
  original's `rand` (MSVC's, seeded from the clock as the original seeds
  it with `GetTickCount`) and hands over after playing to the end of its
  file (`anim.rs`). The faces are drawn on and put back after every
  frame. While a character speaks in a dialog, its first shown slot
  whose id is one of the speaker's `[IdText]` names is hidden, and shown
  again when the speaker changes or the dialog ends (@ 0x00461870; which
  slot when several match is not known [inferred: the first shown one]).
  Not ported: frame lists (+0x2c ≠ 1), state chains, the odd-tick
  flag, `CHEVAL`'s frame variable (no Tibet state uses them).
- **Cursor:** the frames of `spr\interfac.spr` (a `SPRCRYO` bank,
  `crates/atlantis2-data/src/spr.rs`: the shade-run state machine of
  `spr_p5.dll`'s `AFF_SPR` and `AFF_SPRTRS50` (the 50 % mode of the
  subjects already asked) and their tables for RGB565, the last row and
  column of the screen never drawn; the one bank with a 0x400-byte
  (B, G, R, x) colour table, `Ponpirog.SPR`, converted as the DLL does
  for RGB565) at the screen centre: 1..12 over a move zone, 25..39 over
  another zone (none from type 1000 on), 13..24 or 25..39 over a
  clickable object or character, one frame on per tick, chosen after the
  frame is drawn so it shows on the next one. The object in hand (its
  sprite in the world's objects bank, `spr\objtibet.spr` for Tibet) is
  drawn under the cursor; right after a take the cursor itself shows the
  object for one frame. Then the 5-pixel cross of the "centre marker"
  option (on by default) in the colour of what is under the centre
  (zone type or sprite); its two lines are drawn end points included
  [inferred]. The texel under the centre that the sprite test uses is
  taken from the cube's texture mapping (the original projects through
  per-face cameras, @ 0x0047db60, not read) [inferred]; everything after
  it (the box test, the cursor, the hand, the marker, the take) is
  checked against the original frame by frame.
- **Moves:** a click runs the zone click of the original (@ 0x0049ca80):
  a sprite under the centre takes it (an object of click kind 1 is taken:
  hidden for good, in the bag list and the hand); otherwise a move zone
  sets up the move (@ 0x0049cc40): the destination's start and arrival
  angles or the zone's aim, and a turn (@ 0x0045b690 / 0x0045b910) in a
  straight line in (yaw, pitch), 0.2° more per frame up to 2.2°, braking
  over the last 15°, snapping within 0.1°, transcribed with its float
  stores and x87 rounding. A click on a move whose start angle is already
  the view's direction does nothing, as in the original (the first view
  of a new game). When the turn ends the new view is entered at the
  arrival angles and the destination's video starts at 15 frames a
  second (its first frame at once); a click or a double click (two clicks
  under 0.2 s, which also skips the video before it starts) ends it; a
  video on no disc is a cut (14 of Tibet's 16). The arrival is noted for
  the scripts at the first frame after it, after the per-view reset
  (@ 0x00483ce0): `CLICPERSO`, `CLICZONE`, `CLICOBJET` and the object in
  hand cleared, the limits back to ±70° in pitch and none in yaw, and
  that frame's click dropped (no click scan). A click on a sprite sets
  `CLICOBJET` (an object) or `CLICPERSO` (a character) for the click
  scan, which clears them after. With the scripts (below) the departure
  scan runs with `DEPARTVUE` = the target and `ABORTDEPART` (or a section
  that drops the click) ends the click there, `NEXTVUE` replaces the next
  move's view, `UDADA` skips the transition video, `TrajSound` plays with
  the next transition (the music at 20 %), and the arrival scan runs in
  the frame after the arrival; without them (`Engine::new_game`) the
  scans are only recorded as events. A zone click in view 10 runs the
  mandala hook between the departure scan and `CLICZONE` (below). On a
  character with subjects the click opens the subject menu instead and
  runs no click scan (below). Moves to other
  places (type-2 zones), transition sprites and the zoom of a turn are
  not ported (none in Tibet).
- **Mandalas** (`engine/puzzles.rs`): the puzzles' click hook
  dispatcher of the original (@ 0x0049b910) sends a click on a zone of
  type 3, 4 or 50 of a view whose warp name starts with `TI` (any case)
  and whose number is 10 to the mandala hook (@ 0x004954f0), after the
  departure scan and before `CLICZONE` is set (on the sprite path, after
  it); the hook never takes the click. It reads `ObjetEnMain` once, then
  for each of six (object, zone, variable) rows in its order (the Irish,
  Chinese and Yucatán boats on `ZONE_IRB`, `ZONE_CHB`, `ZONE_YUB`, the
  three worlds' mandalas on `ZONE_IRA`, `ZONE_CHA`, `ZONE_YUA`), when the
  zone's name is the row's (byte for byte, case included) and the hand is
  the row's object: `RunOneCommand("(NewSound=400)")` (`sound400.apc` on
  voice 2), `NbMandalaPosed` + 1, and the row's `OrdreMandala1..6` = the
  new count, so each holds the rank its mandala was laid at (the order
  the Shamballa fireflies later test). No guard: the same placement twice
  counts twice (the scripts empty the hand in the same click). Chapter
  75's click scan then does the rest: the `pose..` flag, the laid
  mandala's sprite shown, the hand and the bag emptied, and for a boat
  the world change, the music stopped, the departure film
  (`cine001` / `021` / `040`) and the trip, which stops at its
  `changewam` (Worlds, below). These variables and the show table are in
  the save. Against the original's emulated run (`tests/mandala.rs`,
  `tests/golden/mandala.txt`): the hook on every zone, wrong objects,
  name variants, twice, four whole orders and each mandala laid last; the
  click with chapter 75 for the six placements, an empty hand and the
  crystal (with and without the six flags), line by line up to the
  place change; the dispatcher's choice of the hook for 47 warp names,
  view numbers and zone types. Not ported: the solution log the original prints at exit
  (@ 0x00495650, a fixed list that is not the fireflies' answer) and the
  fireflies themselves (Shamballa).
- **Worlds:** only Tibet (`tibet.wam`, world 5) is ported. A `changewam`
  of the scripts to any other place (the boats' worlds, Shamballa's
  `voiles.wam` after the crystal, `Planetes.wam` of the space travel) is
  refused: where the original loads it (a place it cannot load is fatal
  there), the port notes it (`Event::WorldNotPorted`), ends the section
  at that line, sets the world back to Tibet's (`Interp_SetMonde`: the
  cursor, the objects bank, the texts) and runs the per-view reset of the
  view the game is still in (the hand emptied, as the section's `vue=`
  would), without entering it again. What the section did before stays:
  the flags, the bag, the film played, and the music its `stopmusik`
  stopped (no music until the scripts start one: the original's
  `startmusik` of the same track does nothing either). Clicking the laid
  boat again asks for the trip again (chapter 75's second-trip sections).
  A save whose place is not ported is refused before anything changes.
- **Bag** (`bag.rs`, `engine/ui.rs`): the bag is the scripts' table (90
  entries of sprite, object, world; `sacados`, the take of an object
  sprite, `ChangeObjectWorld`), the object in hand a (bank, frame) pair
  beside the cursor's, `OBJETENMAIN` the object's id. An object's sprite
  is its first `defnames.con` parameter, a frame of the current world's
  objects bank (`objeire`, `objyuc`, `objchine`, `objshamb`, `objtibet`;
  `objyuc` for world 6; `objeire` from start-up until the first
  `monde`). The right button in the panorama opens the bar (`interfac.spr`
  frame 0 at the bottom, the pointer frame 40 at the screen centre; the
  mouse then moves the pointer, one pixel per count, and no longer turns
  the view) and closes it (the hand emptied, the cursor pair kept). The
  bar shows the entries of the current world only (6 counted as 2), eight
  cells of 80 pixels at 640 wide (ten wider), from the first shown entry,
  which a take moves to the end; the pointer within 10 pixels of the
  right edge (5 of the left) and in the bottom 80 rows scrolls one entry
  per 0.1 s; over a cell the object's name of `Messages.txt`
  (`[ObjectNameList]`, keys compared case included, a repeated key
  keeping its last name, a missing one an empty text) is drawn in
  `AtlFT_POPUP` (10 points) centred on the cell, kept on the screen, its
  top at H − 70, black one pixel down and right then white; a click in
  the bottom 80 rows on cell x / 80 puts its object in the hand and the
  cursor, sets `OBJETENMAIN`, stops the turn, uses the click and closes
  the bar. While the bar is open the centre draws nothing, the cursor is
  not chosen, a click takes no object, moves nowhere and runs no click
  scan (a sprite of click kind 3 or 4 still sets `CLICOBJET` /
  `CLICPERSO`). The object in hand is drawn at the screen centre under the
  cursor (hand bank set and frame ≥ 0, frame 0 included), and right after
  a take or a pick the cursor shows the object too until the next cursor
  choice. The world change empties the hand pair but keeps `OBJETENMAIN`;
  the per-view reset empties both but keeps the cursor pair; removing an
  object from the bag keeps it in the hand. Special objects: the dowsing
  rod (`BATON_SOURCE`: `baton.spr` at (W / 2, H − 240) swinging between
  frame 0 and `NEARW77` − 1, one frame per panorama frame, `SOURCIER` = 1,
  the hand empty; put away when the bar opens or `NEARW77` = 0); the
  forms, the fan, the ring and the space-travel balls (`BOULE_SHAMAN*`)
  open an overlay at the screen centre (`form.spr` with the stamps of
  `essai_laby1..3` or the symbols of solution row `soluce_laby` through
  the executable's frame table, `anneau.spr`, the ball's own bank centred
  by its size) that puts the object, the hand and the cursor aside; the
  forms, fan and ring stop the mouse look, the ball does not: it
  animates while the zone under the centre is the one its third
  parameter names (one frame per panorama frame), and a click there sets
  `SpaceTravel` = 0 then 1 and runs a scan (chapter 75 then plays film
  200 and goes to chapter 83 for Tibet's ball); any other click gives
  the object back and closes the overlay. The parchment (`PARCHEMIN` in
  view 10 of an Irish place) needs a byte (@ 0x006d05f0) that no code of
  the original writes: it never opens, here neither. The bag's model,
  the takes of Tibet's boats, the bag and hand keys, the visible counts,
  the scroll to the end and the centre draw of hand and cursor are the
  original's (`tests/objects.rs` against its emulated run); the bar's
  frame, scroll, names, pick, the overlays and the dowsing rod follow its
  code as read (no emulated golden of them yet) and are run with the
  game's files in the same test file. Not known: how `Messages.txt`'s
  reader splits a line (the port takes the rest of the line after the
  blanks as the name, as the shipped names hold spaces); the text's two
  colours (the calls pass 0 and −1, taken as black and white as for the
  subtitles); the frame of a stamp or symbol whose value is outside the
  executable's table (the original reads past it; the port draws
  nothing). The bar scrolls one entry when more than the float 0.1
  (0.10000000149 s) has passed since its last scroll.
  The first frame after the bar closes shows its pointer frame at the
  screen centre, as the original's cursor pair still holds it until the
  next cursor choice.
- **Subject menu** (`subjects.rs`, `engine/subjects.rs`,
  `script/subjects.rs`): the subjects of each character are the scripts'
  table kept as the original's memory (256 records of a count and 16
  entries {value, index, flag}; `enablesuj` appends after the count or
  re-enables, `disablesuj` clears a flag, `killsuj` moves the entries
  after it down, the one past the end included, `clearsuj` zeroes the
  count only), saved as it is. A click on a character's sprite (click
  kind 2) whose count is above 0, with an empty hand, no full-screen
  overlay and the bar closed, sets `CLICPERSO` and opens the menu: the
  cursor becomes the pointer frame 40, the mouse moves it and no longer
  turns the view, the centre shows only its marker, the cursor is not
  chosen and that frame runs no zone click and no click scan. Each frame
  draws the subjects in 62-pixel cells, columns of 4 from x = 450 (the
  first row at 366 − 62 × count below 5 subjects, else at 180; the next
  column only at entries 4, 8 and 12): the subject's frame of the world's
  `suj*.spr` (loaded by `monde`) centred on the cell's (25, 25), at 50 %
  (`AFF_SPRTRS50`, in `atlantis2-data`'s `spr.rs`) when its flag is not 1
  (the scripts disable a subject once asked), then the ring (frame 0)
  centred on (31, 31); then the pointer. A click after the draw answers:
  in the grid of `count / 5 + 1` columns of `min(count, 4)` rows from
  (434, first row − 16), edges included, numbered column by column, the
  entry there sets `SUJET` to its value and the subject scan runs (as
  `CtrlScenarIN`, going on to the next true entry while a body ends at a
  section header: the shaman's sections start a dialog); elsewhere the
  menu closes (`CLICPERSO` = 0, `SUJET` = −2, the hand emptied and the
  cursor hidden). Either way the turn stops and the click is used; after
  a choice the cursor pair still shows the pointer frame at the centre
  in the next warp frame, and `CLICPERSO` / `SUJET` keep their values
  until the dialog's end. A right click with the menu open only stops the
  turn (the original also moves the system's cursor to the window's
  centre, which is not the game's pointer). The original's quirks are
  kept: with 9, 13 or 14 subjects the last column drawn cannot be
  clicked; a click on an empty cell of the last column, or on the
  one-pixel edge after the last row or column, answers the entry the
  memory holds there (past the list, or the next character's record);
  a menu open on a character with no subject (its flag can be set with
  a count of 0: the Yucatán statue's hook does not test the count, a load
  keeps the flag) has a grid of no row, whose top line (y = 350, x
  434..496) answers entry 0, any other click closing it; asked subjects
  stay offered. The cells and the grid for every count,
  the shaman's lists along chapter 75 and the menu in the warp frame
  (opening, drawing, a right click, closing, choices that start dialogs
  and what the scripts see after them) are the original's
  (`tests/subjects.rs` against its emulated run); the same test file
  plays a game that asks both shamans by turning the view, clicking and
  moving the pointer, every question that can be asked before a boat is
  placed, then the astral journey that the scripts' key offers (placing
  a boat leaves for an unported world). Not here: the blackbird game's fixed
  menu of songs (Ireland's puzzle, the mode @ 0x006d2754 = 2), the
  Yucatán statue's hook that opens a menu (`YU` view 122). The menu's
  coordinates are constants of the code, so the original's 800 x 600 mode
  keeps them.
- **Sound:** the voices of the original (0 the music, 1 the dialog
  voice, 2 the films' tracks and the effects) mixed as DirectSound does
  (`sound.rs`). Tibet has no place sound (`tibet.wam` has none and is not
  in the executable's listener table) and its two transition videos have
  no sound track. A film's track (`cineNNN.apc`) plays on voice 2 at the
  effects volume from the film's first frame and stops with it; after a
  film the next panorama frame asks for the music at 100 %.
- **Music** (`music.rs`, the player's states in `engine/host.rs`):
  `startmusik` draws a track from its list as the original does (one
  entry; two, the other one in 85 % of the draws when the current music
  is among them; more, a random index). The player is the original's
  streamer: voice 0 loops over a ring of 0x80000 bytes (5.94 s of 22050
  Hz stereo), created from the music buffer; a start from the beginning
  decodes the track's first 0x80000 bytes there (and seeds `rand` with
  the 15 Hz count), a start at a mark (a given one or one drawn at
  random) restores the `.spp` record's decoder state, moves the write
  offset back 0x14000 bytes, decodes four 0x4000-byte chunks there and
  puts the play cursor where they start. On each pass (`check_buffer_music`
  @ 0x00477590, in the playing and fading states) a chunk of 0x4000 bytes
  (0x1000 file bytes) is decoded at the write offset once the play cursor
  has left it, so the ring stays about 5.8 s ahead of what is heard. At
  the end of the data the end-of-track callback draws the next track from
  the list (its start when the list has one entry), which opens and goes
  to its mark behind the cursor: what the ring still held of the old
  track is never heard, the decoded bytes ahead of the cursor (the ring
  less the chunk the cursor has just left and what it has played of the
  next, a little under 0x7c000 bytes) and the track's last partial chunk
  (up to 0x4000 bytes): 511740 to 521212 bytes, 5.80 to 5.91 s, in the
  runs of the original under emulation. With an empty
  list, or with loops left (`*n`), the original rewinds the file, but the
  rewind's check compares the PCM size with the file's, fails, closes
  the file (and copies it again): the ring loops on, the track's last
  0x7c000 bytes or so with a fragment of what the ring held one turn
  before, until another music command. Native does both as the original
  (`tests/music_player.rs` replays the original's runs under emulation,
  with a play cursor moving 0x1000 or 1468 bytes per pass: every event,
  position and md5 of what is heard), unless the minor bug fixes are on
  (the default, see [Enhancements](#enhancements)): then the next track,
  or the rewound one, is decoded right after the old one's last sample in
  the same chunk, and every track is heard to its end. A music playing
  first fades out; a new one starts at the music volume (14 in a dialog);
  `stopmusik` fades out and stops; the films (20 %) and the dialogs
  (16 %) duck it with the fade machine of the dialogs (127 units a
  second). The files are read from the discs (the install's `music\`
  copies are empty until `LOADMUSIK`, which, like `delmusik`, does
  nothing here). Every track decoded from its start and from each of the
  83 marks, the random marks and the list choices are the original's
  (tests/music.rs). Known differences: the music player shares the C
  runtime's `rand` with the dialogs, which the original also shares with
  the place animations (two generators here).
- **Alpha masks:** the dialog videos carry an `AL` chunk per frame (runs of
  (n, a) pairs, a = 0..31), which the decoder keeps for the caller
  (`HnmVideo::alpha`) and `AlphaTables::blend` draws as the original's
  dialog video stream does: the frames are premultiplied, a = 31 copies
  the video, a = 0 keeps the background, and in between the background is
  scaled by (31 − a) / 32 with the original's tables.
- **Dialog heads** (`head.rs`, `raster3d.rs`, `lipsync.rs`): a speaker's head is a `.db` scene written by Cryo's 3ds Max
  exporter (`crates/atlantis2-data/src/db.rs`: meshes, a node hierarchy
  with key-frame tracks, materials, the mouth shapes as morph targets,
  cameras) with a 512x512 16-bit RLE TGA texture and two eye-blink maps
  (`tga.rs`). The scene is posed at the body video's frame number, its
  mouth mixed from the mood's 7 visemes, and the head drawn by a
  transcription of the original's C pipeline: the object → camera matrix,
  the transform and clip codes, the back-face test, the projection, and
  the rasteriser `Zi32Texture1516FastPerspective1516C` (a fan of
  triangles, perspective every 32 pixels, nearest texel, a 32-bit Z
  buffer), every float store and x87 rounding as the original's. The eyes
  blink at random (2 % a frame, then three frames closed, half closed,
  open), with the C runtime's own `rand`. The lip-sync analyses the voice
  by blocks of 256 samples (energy and a histogram of zero-crossing
  intervals) and turns it every frame into the 7 weights through the
  speaker's 10x7 `[Matrice]` of `scenar\info<world>.txt`, smoothed by
  `[Smooth]`, `[Offset]` seconds ahead of the voice (`dialog.rs` reads
  the scripts). Frames, Z buffers, intermediate buffers and the weights
  of every Tibet line are the original's, bit for bit, at either x87
  precision for the lip-sync; `cargo run --release --example head_png`
  draws a head to a PNG.
- **Dialogs** (`dialog.rs`, `hnm_stream.rs`, `subtitle.rs`, `sound.rs`):
  a world's `loc\Dial<XX>.txt` (the texts, Windows-1252),
  `scenar\com<XX>.txt` (speaker, mood, stand, camera, kind of each line)
  and `scenar\info<XX>.txt` (the speakers) are read with the world; a
  dialog collects its lines, loads each speaker's head per stand, and
  picks each line's camera as the original does (given, kept from the same
  speaker's previous line, else drawn with `rand`). On the 15 Hz tick a
  line plays until its voice (`apc\dial\NNN_MM.apc`, decoded whole) has
  played, or a click; its body video (`hnm\dial\<mesh>s<stand>c<camera>.hnm`)
  shows frames 1 to its last in a loop (the original's player decodes two
  frames when it opens one) and the head is posed at the same frame number;
  the panorama takes the speaker's field of view for the line's camera,
  levels its pitch at the first line, and turns by π whenever the hero
  takes over or leaves the word (reverse shots); a playing music fades to
  16 % at 127 volume units a second and back to 100 % at the end (with no
  music playing the original refuses the fade, and so does this). A left
  click goes to the next line. Each frame
  draws the panorama, the body video through its mask, the head with the
  lip-sync of the voice's current time, and the subtitle. The subtitles
  break and place their lines as the original's code does (a byte with the
  high bit set is taken with the next one, so an accented letter before a
  space is no break; lines 12 pixels apart, in white over a black copy
  shifted by one pixel), with the widths of the game's own font,
  `loc\Atl_Font.ttf`, from its `hdmx` table at 16 pixels per em and its
  ascent from its `VDMX` table, as GDI uses them. The glyphs are the
  font's outlines filled where at least half covered: GDI also runs the
  font's hinting, which this does not, so a few pixels of a glyph differ
  from the original's (the one known difference). The voices are mixed as
  DirectSound mixes its buffers (gain 10^(attenuation / 2000)). With
  `cinema=N,M` dialog M plays over film N: at each video tick its line
  sequencer alone (no click, no camera change; its end is ignored), then
  its draw over the frame, the dialog removed when the film ends (FUN_0040c030;
  what the dialog step draws there is the dialog frame's own rule, no
  Tibet film has one). Not there yet: the `voixoff0` and `cinema` lines'
  pictures. The lines, the
  video frames and their numbers, the composed frames, every subtitle's
  layout and the fades are the original's (tests/dialogs.rs).
- **Scripts:** the chapter scripts `scenar\chapiNNN.con` keep the first
  game's text language but not its interpreter: names come from
  `scenar\defnames.con` (five sections, looked up per key:
  `crates/atlantis2-data/src/defnames.rs`), compares are signed, a section
  has a head condition and sub-conditions, and the text is walked byte by
  byte with the original's quirks (a `#/set` before an empty line runs,
  `KEY=++` stores 0, two kinds of line run twice in `/INIT`). The
  interpreter (`crates/atlantis2-engine/src/script/`) is transcribed walk
  by walk: conditions, the `/set` handler with the subjects, the bag, the
  show/hide table and the hand, the scans, `/INIT`, the timers, the 15 Hz
  clocks (`TIME1` and `TIME2` count seconds of real time; at each second
  `RANDOM` takes the 15 Hz tick count & 0xff) and when
  each scan runs in the panorama frame. What a command does outside those
  tables (a place, a view, a film, a dialog, music, a disc, a menu) goes
  through its `Host` trait, one method per function of the original. The
  engine is that host (`engine/host.rs`) and runs the interpreter where
  the original does: start-up and the menu's new game
  (`Engine::boot`, `Engine::start_new_game`), the 15 Hz clocks, the warp
  frame's sprite click (`CLICOBJET`, `CLICPERSO`, the take), departure
  scan, `Interp_GereFrame` (the arrival scan after the per-view reset,
  `ABORTDEPART`, the angles, the timers) and click scan, the arrival of a
  move (`LASTVUE`, `VUE`, `ARRIVEEVUE`), and the ends of the video and
  dialog loops, which let an interrupted section go on. The script's
  limits are read by the mouse look every frame; the start angles of the
  next view entry and the click flags are one state shared with the
  navigation; the object in hand is the script's. The hooks wired:
  `changewam` (any place file), `vue`, `nextvue`, `anglewarpx/y`,
  `anglewarpxnow/ynow` (the camera set to the angle [inferred: the
  original sets the scene node's rotation]), the four zone keys (the type
  of the zones of that value in the first such view of the current place
  ± 88888), `ABORTDEPART`, the show / hide table and the animation keys on
  the view's slots (`hideperso`, `showperso`, `hideobjet`, `showobjet`,
  each on the first matching slot only,
  `PLAYANIMP` / `O`, `CONTINUEANIM`, `STOPANIM`, `GOTOFRAME*`,
  `ADDFRAME*`, `ChangeAnimGotoPerso`, and the `NUMFRAME*` conditions;
  only the first four occur in Tibet, the others follow the original's
  handlers of the other worlds and no Native test checks them yet), `cinema` (the film at 15
  frames a second with its sound track, the music at 20 %, dialog `M` over
  it), `dial` (the dialog loop; a failed start leaves the section waiting,
  as in the original), `startmusik`, `stopmusik`, `NEWSOUND`, `TrajSound`,
  `monde` (the disc rule, the objects bank, the dialog texts), the bag and
  the hand (`sacados` scrolls the bar to its end, `objetenmain=0` also
  hides the cursor). No-ops, logged: `ZoomStart`, `ZoomPsychoStart` (the zoom of
  a move), `ANIM_THE_FRAME` (the playing byte), the special events
  (`TestSpecialEvent`), `LOADMUSIK`, `delmusik`, `CHANGECD` (every disc is
  read where it is), `MESSAGE_CLIC`, `ViewBigSpr` (`CREDITS` shows the
  credits); `NextWarpInvalide` is stored but the view entry does not read
  it; `SHOWGUEST`, `INCPERSO`, `ADDGUEST`, `SUBGUEST` and the plain
  variables are script state only, as in the original. A fatal error of
  the scripts (an unknown name, a missing chapter) stops them and is
  logged; the game itself goes on. The original's own unknowns stay open:
  how the menu resets the variables of a new game (the
  port starts from the start-up's tables), and whether its first scan is
  a click (the port follows the code: `vue=2` sets `ARRIVEEVUE` to 0, so
  no arrival scan, and the first click runs chapter 0's first section,
  which loads chapter 75).
  `tests/play.rs` runs a new game headless from the start-up through the
  film, the music, the first click, the walk to view 7 and its dialog to
  view 10, and compares the lines the scans run with the original's
  (`scripts_con_tibet_scan.txt`: lines 14 and 15 around dialog 360, line
  294 in view 10) and the dialog's lines with `dialog_script_ti.txt`.

- **Menus** (`crates/atlantis2-engine/src/menu.rs`, the engine's side in
  `engine/menus.rs`): the original's pages are built in code (loop type
  5): a 640x480 background (`spr\menu\f_*.tga`, RLE or not, 16 or 24 bits:
  each channel keeps its top bits, as the heads' maps) or the game screen
  at half brightness, and widgets of `spr\menu\*.spr` at fixed positions
  with the tooltips of `loc\Menu.txt` (sections padded with their last
  line when short). The widget classes are the original's: sprites,
  buttons (the box frame 0's size centred on the position, strict
  inequalities), hover buttons (frame 1 under the mouse), animated
  buttons (20 frames, one per 15 Hz tick while the mouse is in the box
  with the button up, while pressed by a click in the box until the
  button is released, or always), two-state and blinking toggles, the volume sliders (gauge and
  knob, x0 = x − 65, 115 pixels, the action every frame while held) and
  the load page's list (60-pixel rows, the thumbnail of `save.spr` and the
  date and time in `AtlFT_POPUP`, newest first) with its scroll bar (at
  the list's right edge, a 200-pixel track). A tooltip shows after 10
  ticks in a box, centred on it in white over black. Each frame the
  widgets are asked in order and the first action wins; the state machine
  then slides the main page's panels (1 + 2 + ... pixels a frame: 30
  frames for the options and the disc, 40 for the sound), asks its
  questions in the yes / no box (save, erase, quit), opens the pages, and
  sets the options (subtitles, centre marker, inverted mouse, the screen
  size, which is kept in `atlant2.cfg` but not used: the port draws
  640x480) and the volumes (general = slider × 256, music = music slider
  × general slider × 127 + 0.5, effects and dialogs × 127, truncated).
  Escape closes the main menu opened from the game (not the player menu).
  The cursor is `curseur.spr` at the mouse; the window then lets the
  pointer go and draws the game's own. Quitting shows the credits, as
  does the scripts' `CREDITS` (`crates/atlantis2-engine/src/credits.rs`,
  checked frame by frame against the original under emulation,
  `tests/credits.rs`): the pages of `loc\Credits.txt` (21 in the French
  release) over the 16 pictures `f_cred01.tga` .. `f_cred16.tga` in turn,
  each line one step (the popup font's height in points) below the last,
  a `#` line centred on x = 522 (at 640 wide) and any other at x = 405,
  white over a black copy one pixel down-right, written on the picture so
  they fade with it. Each page shows 5 s, then fades into the next over
  2 s (weight ftol((t − 5) × 127.5) of the time since the page started,
  under the x87 control word), the change coming at the frame after the
  fade ends. The time is the original's counter quotient (the high dword
  times 4294967295.0 plus the low one, over the frequency, rounded as the
  control word says) on the port's own counter: the host clock in whole
  milliseconds at the frequency 1000, the harness's clock (the original's
  counter is the machine's: the time-stamp counter at the processor's
  frequency when it has one, else the performance counter; under the
  24-bit control word its bits, and so a weight or a page change a frame
  apart, depend on that machine); the last page fades to black and the credits end one frame
  after that (21 cycles of 7 s and a frame: 147.35 s at 60 frames a
  second), or at a click or while Escape is held (a key state, not a key
  press). The fade is the original's table blend: every channel of a
  picture shows one step darker than in its file, green keeps five bits,
  and a strong blue takes one from green (the signed shift of the
  original). A 24-bit picture would be copied unchanged (checked on
  synthetic files; the shipped ones are 16-bit). Their texts are drawn
  with the game's TrueType font as the subtitles are (the original's are GDI's: a few pixels of each glyph
  differ). Built but not opened by the
  game: the disc request (every disc is read where it is), "please wait",
  "disk full" and "write error" (the original opens them only while
  `LOADMUSIK` copies music, which the port does not do). Not known, and
  chosen here: whether the page title (line 0 of each section) is drawn
  (it is not), what
  the 800 x 600 mode does with the 640 x 480 pages (centred). The list's date and
  time are written in the French system's formats (`22/10/2026
  14:12:00`); the original asks Windows for the user's.
- **Saves** (`crates/atlantis2-data/src/save.rs`, the engine's side in
  `engine/saves.rs`): the original's format, so its saves and the port's
  are the same files: `save\ATL<player>_<slot>.SAV`, 99 slots per player,
  in the `atlantis2` folder of the port's data folder. A save is the 35
  globals of the original's table back to back (69288 bytes: the place,
  chapter, view, time and thumbnail of the header, the 300 variables, the
  1024 flags, the bag with its unused entries in their initial pattern,
  the camera's angles, the music and its list, the show / hide table, the
  cursor frame, the puzzles' states, the subjects) in a `ZapFile` (its
  run-length encoding transcribed: a literal of 10 bytes at least, a fill
  run after each, even empty; the bytes the original leaves to chance
  written as 0). A raw file of 69288 bytes loads too and is saved again
  compressed. Saving writes the player's first free slot; the load page
  lists the valid ones newest first and can erase one. A load does what
  the original's does: the 35 globals back, the chapter's script loaded,
  the world set (the hand emptied), the place loaded and the saved view
  entered at the saved angles with no arrival scan and no per-view reset,
  the saved music (the music ring restarted at a drawn mark, after a fade
  when another track plays), the dowsing rod (shown, frame, step;
  `baton.spr` loaded again when shown), chapters 39, 46 and 83 run their
  `/INIT` again. The puzzles and the rod's entry 15 (@ 0x006d84f4, role
  not read) are kept as the bytes of the last load until their studies
  are ported; the dialog flag is written from the
  running dialog and not read back (a save made from the menus has no
  dialog running: Escape ends it before the main menu opens). Chosen here: a section
  waiting to go on and a queued command are dropped at a load (the
  original's flags are not saved: what it does with them is not known);
  a save is stamped with the local time, as the original's; the subject
  table is saved as the memory holds it, the entries past a count
  included. `atlant2.cfg` (11 options, 35 bytes) is
  read at start and written at exit. The encoder, the decoder, a slot's
  file byte for byte and the options file are the original's
  (`crates/atlantis2-data/tests/zapfile.rs`); the menus, a save and a load
  through them, the newest save chosen by the player menu and the credits
  are run with the game's files (`crates/atlantis2-engine/tests/menus.rs`).

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
installation, for the tests of those builds. For Atlantis II,
`ATLANTIS2_DATA` names the installed folder (`atlantis2.exe`, `wam\`,
`scenar\`...) and `ATLANTIS2_DISCS` a folder of its four CDs' images (or
copies); the expected frames, matrices, zone picks, heads, lip-sync
weights, dialog lines, dialog frames, subtitle layouts, music fades,
script results and save files come from the original `atlantis2.exe` run under emulation
(Tibet's moves pass by pass, the place animations tick by tick, the
screen centre and the clicks on sprites pass by pass with the texel under
the centre given, every SPW frame of Tibet and the first of every other
file, every `AFF_SPR` and `AFF_SPRTRS50` of the installed banks, the
subject menu's cells and click grid, the shaman's subjects along chapter
75; names of `defnames.con`
entries are written by section and index in the script files and filled
in from the game's file by the tests):

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

Atlantis II, on the `a2/m0` branch:

| Milestone | Content |
|---|---|
| M0 | detection, the file layer, the cube renderer: a new game shows the first Tibet panorama, checked against the original's frames (done) |
| M1 | Tibet playable: navigation, scripts, dialogs with the 3D talking heads, inventory, menus, saves |

