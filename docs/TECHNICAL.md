# Atlantis Native: technical notes

[Back to the README](../README.md)

## Layout

| Crate | Content |
|---|---|
| `atlantis-data` | BigFile archives, executable tables, game detection |
| `atlantis-codecs` | HNM6 panoramas and videos (with Atlantis II's alpha masks), APC sound; shared by both games |
| `atlantis-engine` | the game, independent of any platform: input events in, 640x480 frames and audio out |
| `atlantis2-data` | Atlantis II: detection, the install folder and the four discs, `WAM V2.8` places, `defnames.con`, the place sprites (SPW) and the sprite banks (`SPRCRYO`), the `.db` 3D scenes and head maps, the dialog scripts, the object names of `Messages.txt` |
| `atlantis2-engine` | Atlantis II: the cube panoramas and their camera, the place animations, the cursor and the moves between views (Tibet), the dialogs (lines, voices, body videos, 3D heads, lip-sync, subtitles, music ducking), the CON script interpreter, the menus and the saves, the bag bar, the object in hand and the special objects' overlays, the Tibet mandala puzzle, Ireland's four puzzles (Brigid's cross, the skull, the ogham stones, the blackbird songs), China (the zoom of a move, the picture viewer, `NextWarpInvalide`, the demons' special events, the figurines on the floor, the chest, the five wheels, the pearl and the dragon, the turtle ring, the labyrinth's souls: all seven of its puzzles), Yucatán and Xibalba (the Maya counting grids, the Tezcatlipoca statue's hook, `AngleWarpXNow` / `AngleWarpYNow` as the original turns the view; Xibalba with all three of its puzzles: the star map, the 3D canoe puzzle, its own loop type 9 with the Atl3D database pipeline (`atl3d/ortho.rs`, `canoe.rs`), and the spider's web with its 3D creatures over the panorama (`atl3d/warp.rs`, `engine/web.rs`)) |
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

Atlantis II has its own `Enhancements` (`crates/atlantis2-engine/src/enhancements.rs`):
`ui_ux`, on by default, holds the conveniences the original lacks (the
arrow keys turn the view; the click that gives the window the focus only
takes the pointer back), and `minor_bug_fixes`, on by default: every music track is heard
to its end, the next one of the `startmusik` list starting right after
its last sample (the original drops the last ~5.8 s of a track followed
by another), and a track that ends with an empty list, or with loops
left, plays again from its start (the original loops the last 5.9 s of
its ring until another music command), and a random mark drawn at the
`rand()` = 0x7fff edge (one draw in 32768 of a `startmusik` entry
without a mark) plays the track's last mark (the original's draw is one
past the marks, a fatal error: it saves and quits); see the Music item of
[Atlantis II](#atlantis-ii-in-progress). With it too, in Ireland's two
puzzle views (Brigid's cross, view 2; the ogham stones, view 6) a click
on a puzzle zone runs its puzzle and nothing else: the original also
reads the zone's plain value as the index of a named zone, so that in
chapter 12 the fourth stone sends the player back to Tibet as the
weeping head does, and in chapter 14 the ninth, with the horse in hand,
plays the water's film (see the Ireland item of
[Atlantis II](#atlantis-ii-in-progress)). `--original` turns it off there
too. It also holds the [HD mode](#hd-mode) (`hd`, off by default, and
`hd_options`, the first game's `HdOptions`), which `--hd` and
`--widescreen` turn on; see the HD mode item of
[Atlantis II](#atlantis-ii-in-progress).

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

Atlantis II has the same HD mode for its cube panoramas, its dialog
heads and its texts (`--hd`, `--widescreen`; see the HD mode item of
[Atlantis II](#atlantis-ii-in-progress)).

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

A folder holding `atlantis2.exe` (or GOG.com's installation, with its
`Game.exe`) opens Atlantis II (`crates/atlantis-app/src/a2.rs`):
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
hut (below). Ireland's boat laid there leaves for Ireland (world 1,
below), which plays to its end with its four puzzles (Brigid's cross,
the skull, the ogham stones and the blackbird songs, below) and sends the
game back to Tibet with Ireland's mandala. China's boat leaves for China
(world 3, below), which plays to its end with its puzzles (the
figurines, the chest, the incense, the five wheels, the pearl and the
dragon, the turtle ring and the labyrinth's souls, below) and sends the
game back to Tibet with China's mandala (the lantern), or at once by its
gong. Yucatán's boat leaves for Yucatán (world 2, below), which plays
with its counting grids and sends the game back to Tibet by its brazier;
the priestess's yes leads into Xibalba (world 6, below), whose three
puzzles (the star map, the canoe, the spider's web) are played, and the
soul it gives ends Yucatán with its mandala. Each world's ball (the
shaman's) leads to the space-travel hub and back (below). The six
mandalas laid, the crystal laid in the hut leads to Shamballa and
Atlantis (world 4, below), which play to their end with the fireflies
(the six lit in the order the mandalas were laid) and the octopus fight;
the octopus's death sends the game back to Tibet, and the
crystal laid again to the hub's last view, the films, the credits and the
end of the game.
`--headless <folder> [frames] --screenshot
<file.png>` plays a new game for that many passes (59.94 a second) and saves the
screen;
`--view <id>[,<yaw>,<pitch>]` starts in another view, `--dialog <n>`
plays a dialog at once and `--menu <page>` shows a menu page (`player`,
`main`, `options`, `sound`, `load`, `credits`; testing aids, which go
through the new game's music copy page when the data folder holds no copy
yet, then end the opening film first),
e.g. `--view 7,-164.394,14.96 --dialog 360`; `--take <object>[,...]` puts
objects in the bag and `--hold <object>` one in the hand (by their
`defnames.con` names, through the scripts' `sacados` and `objetenmain`),
`--bag` opens the bar, `--pointer <x>,<y>` places the mouse pointer and
`--talk` clicks the character under the centre (e.g. `--view 7,70,5
--talk --pointer 475,329`: the shaman's menu, the pointer on a subject);
`--command <commands>` runs script commands before the view entry (e.g.
`--command "(monde=1)(changewam=bigile1.wam)(vue=1)(chapter=3)"`: Ireland,
`--command "(monde=3)(changewam=chine.wam)(vue=10)"`: China,
`--command "(monde=2)(changewam=yucatan.wam)(vue=10)"`: Yucatán,
`--command "(monde=6)(changewam=xibalba.wam)(vue=122)(chapter=39)"`:
Xibalba's statues); `--seed <n>` gives the C runtime's `rand` the
start-up seed n instead of the clock (the same draws on every run); `--hd`,
`--widescreen` and `--size <w>x<h>` give the HD mode at that window size,
and a headless run's screenshot is then the window's picture at that size
(and its HD timings are printed). `Engine::new_game` keeps the
start without the scripts (the view `/INIT` names) for the tests against
the original's moves. The example `cargo run --release -p atlantis2-engine
--example tibet_tour <install> <discs> <out>` writes pictures of three
views with their animations and a recorded walk from the first view to
view 4; `--example menu_png <data folder> <prefix>` draws the menu pages
after a short game with three saves; `--example ireland_tour <install>
<discs> <out>` lays Ireland's boat and draws the trip (the departure
film, the arrival's dialog, Liam and his subject menu, a transition
with its sprites, the cross's view, the cross laid on its place, the
goddess, the way back to Tibet) into `ireland-*.png`; `--example
china_tour <install> <discs> <out>` lays China's boat and draws the trip
(the departure film, the arrival, the gardener master's menu and answer,
the turtle room and its picture, the miniature garden and its map, a
move's zoom, chapter 59's door, the labyrinth and a demon's special
event, the gong's way back to Tibet) into `china-*.png`; `--example
yucatan_tour <install> <discs> <out>` lays Yucatán's boat and draws the
trip (the departure film, the arrival, the king's hall turned by
`AngleWarpXNow`, a character's menu, the priestess, the pyramid's first
grid with marbles laid, the Tezcatlipoca statue's menu, the brazier's way
back) into `yucatan-*.png`.

- **Builds:** `atlantis2.exe` of the French CD (1.00, version 3.15, 1999,
  2199552 bytes, PE timestamp 0x38030926), the official 1.01 patch's
  `atlantis2.exe` (version 3.18, 2183168 bytes, PE timestamp 0x38328ed1,
  one file for the French and the English releases) and GOG.com's
  `Game.exe` (the 1.01 executable with 18 byte patches of GOG's),
  recognised by size and timestamp and confirmed by MD5
  (`crates/atlantis2-data/src/exe.rs`). GOG's folder holds an
  `atlantis2.exe` too, a 20,992-byte splash-screen launcher: the
  executable is the first of `atlantis2.exe` and `Game.exe` that is a
  known build. 1.01 moved most of the code and data; the one table the
  port reads from the executable, the place sounds' listener table
  (@ 0x005dd2c0 in 1.00), is read at each build's own address (0xf60
  lower in 1.01, the same 5780 bytes). What 1.01 changes that a player
  sees, reproduced when a 1.01 executable is found (GOG's or a CD
  installation patched to 1.01), the CD's 1.00 keeping its own behaviour:
  - `loc\Font.txt`'s new `[CharacterSet]` section (`SBCS` / `MBCS`):
    only with `MBCS` is a byte with the high bit set a lead byte, so with
    the French files (no such section) an accented letter is a character
    of its own and the space after it a break; 1.00 always glues it to
    the next character;
  - a font's `Height` is a cell height (`CreateFontA(+Height)`, no longer
    −MulDiv(Height, 96, 72)): GDI takes the largest size whose `VDMX`
    height fits (GOG's `Font.txt` asks 22 for the dialogs: 15 pixels per
    em, a 21-pixel cell; 16 for the popups: 11 pixels per em; the CD's 12
    and 10, below the font's smallest `VDMX` cell, give 7 pixels per em
    in a cell of the height asked: subtitle lines 13 pixels apart). These
    sizes are Windows' own, measured on Windows 11 and Windows 98 SE
    with `GetTextMetricsA` at every height from 2 to 40; Windows 98
    differs only below the smallest `VDMX` cell (a 9-pixel cell whatever
    the height), and the port follows Windows 11, where the GOG edition
    runs;
  - the subtitles' and the credits' lines are `GetTextMetricsA`'s
    `tmHeight` + 1 apart (22 pixels with GOG's files), no longer
    `Height`;
  - a font file that is missing is replaced by a system font (1.01 logs
    it; 1.00's `AddFontResourceA` fails and GDI substitutes a face the
    same way): here the port's Liberation Sans;
  - Alt+Enter, which switches 1.00 between full screen and window (the
    main Enter while the left Alt is down: the key table's modifier slot
    is `DIK_LMENU` once DirectInput runs), does nothing (F11 is the
    port's own key, with both).

  GOG's own patches change how the program talks to Windows (its install
  folder taken for the CD, no free-space check before the music copy, no
  `WM_FONTCHANGE` broadcast, DirectInput re-acquired on any error,
  `WM_ACTIVATE` for the window's activation), which the port has no use
  for, and three things a player could see: the fonts are drawn without
  antialiasing (`NONANTIALIASED_QUALITY`), as the port draws them with
  every build; a video ends at its frame count only, as the port's do;
  and the CD page, should it open, closes at the first 15 Hz tick without
  looking for the disc, the current disc unchanged, which the port does
  with GOG's executable. The subtitles of 1.01 are checked against its
  own subtitle function run under emulation on the CD's and on GOG's
  files (`golden/subtitles_101_cd.txt`, `golden/subtitles_101_gog.txt`,
  `tests/dialogs.rs`), with the font sizes and widths measured on
  Windows 11 as the harness's GDI.
- **Files:** the install folder holds the places, scripts, texts and
  sprite banks; everything under `datas_a2\` comes from the current CD.
  The discs (images or copies) are found in the game folder, in the
  folders directly inside it and in each `--discs <folder>`, and told apart
  by the original's marker files (`datas_a2\spf\sphex.spr`, `cerceris`,
  `bousier`, `mante`). The current disc follows the `MONDE` rule (worlds
  1..4 on discs 1..4, 6 on disc 4, Tibet on the disc of the first drive
  holding one); Native plays as one drive whose disc is changed when the
  game asks, so Tibet keeps the disc already in. Six files differ between
  the discs, so a file is read from the current disc first
  (`crates/atlantis2-data/src/files.rs`). Other files missing from the
  current disc come from another one, except the sounds of
  `datas_a2\apc\`, read from the current disc only as the original
  reads them: film 44 (chapter 27, disc 4) has its track on disc 2 only
  and plays silent, as in the original; film 62 on the way back from
  Shamballa (chapter 78, `monde=5` with disc 4 in) plays its track from
  disc 4. The discs' long names are
  in their Joliet trees, which the disc-image reader now reads. GOG's
  installation holds every disc in one `DATAS_A2` folder of its install
  folder, with the four discs' markers: a folder (or image) holding all
  four counts as each of the discs 1 to 4, so no disc is ever asked for
  and the music is copied from it as from the disc of the world (the
  original would take it for disc 4 alone; GOG's executable is patched to
  use it for every disc). Its data is the European DVD's: upper-case
  folder names (no matter, the lookup ignores case), one copy of each of
  the six files that differ between the discs, 8.3 copies beside the long
  names, and the ten file names with accents of the discs only without
  their accents (`TR-D-tete araignee.tga`) or as 8.3 names
  (`TR_B_TET.TGA`), while the spider's and the canoe's `.db` files name
  them with their accents: a name not found is looked for with its
  accents folded, then as its ISO 9660 level-1 name. Film 44 has its
  track there, so it is not silent as on the CD. GOG's texts are a
  corrected edition (spelling and typography), and its chapter 83 plays
  dialog 398 over film 66 (`/set(cinema=66,398)`, a line the CD's texts
  hold but its script never shows); both come from the data.
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
- **HD mode** (`crates/atlantis2-engine/src/hd.rs`, `engine/hd.rs`; the
  window's side in `a2.rs`, `Picture`): the first game's
  [HD mode](#hd-mode), its layer, options, panorama filters and
  composition reused (`atlantis_engine::hd`, `atlantis_engine::upscale`).
  It is `Enhancements::hd` (off by default; `--original`, which turns the
  bug fixes off, leaves it as `settings.ini` has it, as the first game's
  `--original` leaves its graphics mode), turned on by `--hd`, with the widescreen option by `--widescreen`
  (`hd_options.widescreen`), or by the options menu (F10, below), which
  keeps it in Atlantis II's `settings.ini`. With
  it off the engine does none of this. With it on, the engine's 640x480
  frame is still the original's, pixel for pixel (the scripts, the picks
  and the goldens see the same frames; a test plays the four worlds' views
  both ways); after the cube is drawn, the same view is drawn again at the
  window's resolution from the same faces (the place animations in them)
  and the same camera floats, with the 640x480 cube it stands for (the
  layer's `base`). The size is the first game's: 40 x 30 tiles of the
  picture's width / 40 pixels rounded up, at most 64 (2560x1920), for
  any picture wider than 640 pixels (`tile_for_output`: 960x720 has tile
  24, the default 1280x960 window tile 32); only a window of 640x480 or
  less shows the 640x480 frame alone.
  The view evaluates the original's projection at each of its pixels: the
  original projects a camera point to sx = fx·x/z + cx, sy = fy·y/z + cy
  (fx = fy = 640 / (2 tan 38°) = 409.58, (cx, cy) = (320, 240),
  `FUN_004a49c0`, `FUN_0045c030`) and samples its pixel i at i + 0.5 (the
  0.5 the set-up adds, @ 0x0057b3c7); the HD pixel (X, Y), at s = tile / 16
  times the size, is the screen point ((X − side + 0.5) / s, (Y + 0.5) / s),
  its ray ((sx − cx) / fx, (sy − cy) / fy, 1) is turned to the world by the
  camera's matrix as `FUN_0045adf0` turns the centre's, and the face it
  meets gives the texel coordinates 1 + 510·t of the corners' 1/512 and
  511/512 (`FUN_0045c790`; the same mapping as the sprite pick's texel
  under the centre). Tests check it: at the screen centre the ray is the
  camera's (the zone and sprite picks); for 74000 screen points on 168
  views the cube point projects back by the original's formulas to the
  point (within 0.001 pixel), past the 640 columns too; drawn with the
  original's nearest texel at the original's pixel centres, the view is
  `warpPII`'s frame in 82.75 % of the pixels on the synthetic faces whose
  pixels name their texel, the others one texel away (83 % of them lower:
  its 16-bit steps truncate) but for 740 pixels in 103 million two texels
  away, where its affine spans (a division every 32 pixels) stray most;
  `warpC` is the same in 87.06 % of them (310 pixels two texels away; a
  test checks each). Where the original takes the nearest texel, the view
  samples the faces bilinearly, by default bicubically (the faces
  doubled with Catmull-Rom) and sharpened, the first game's filters
  (`PanoramaPrep`, on the six faces with a border of 4 texels of each
  face's own edge, so that no filter reads the next face; only the tiles a
  sprite's frame changed are computed again). Widescreen: in a window
  wider than 4:3, tile columns of view on each side (at most 16 a side, a
  2.4:1 picture), screen points past 0 and 640 on the same formulas: a
  wider horizontal field of view (92° at 16:9, against the original's
  76°), the vertical one (60.7°), the angles and their limits unchanged;
  the centre is the same, so is what is under it. What the original draws
  over the cube (the cursor and the object in hand at the centre, the bag
  bar, the subject menu, the overlays, a dialog's body video, the
  subtitles) and the frames without a cube (the films, the transition
  videos, the menu pages) are its 640x480 pixels, at their places and in
  their order: the first game's `HdCompositor` shows the HD view wherever
  the frame still shows the cube (`base`), and upscales the rest, xBR for
  the art and bilinearly for a video's frame (black bars beside a video or
  a menu page); they cannot be improved (pictures made at 640x480).
  The dialogs, as the first game's HD heads: a head line's frame
  (@ 0x00461430) draws the body video through its `AL` mask over the
  panorama, then the head; the engine keeps the picture under the body,
  the body's pixels (alpha not 0), the frame before the head and after it,
  and draws the head a second time into a frame of 40·tile x 30·tile
  pixels with the same pipeline (`Head::draw_scaled`): the same pose,
  mouth, blink texture and control word (the one the original draw started
  under), the camera's projection scaled by s = tile / 16 (fx, fy, cx, cy
  times s, so the camera point the original projects to (sx, sy) goes to
  (s·sx, s·sy), the frustum slopes of @ 0x004a4e8d from the larger
  viewport), the vertices, back faces, Z rule (2³¹ / z, the later face
  winning a tie) and texture those of the original, and the rasteriser
  of the variant (the Pentium II's or the C one) with its viewport the
  larger frame; it samples the nearest texel at the larger frame's pixel
  centres. Tests check it: at tile 16 it gives the original's frame and Z
  buffer bit for bit (three heads, both variants, every camera, 14 poses);
  at tile 48 the HD pixel at the centre of each original pixel is the
  original's texel in 98.7 to 99.3 % of them (the 32-pixel perspective
  steps and the fixed point fall elsewhere over more pixels), no HD pixel
  lies off the original's footprint and its neighbours, and the head has
  nine times the pixels; the original's draws after an HD draw are those
  of a head never drawn in HD (frames, Z buffers and control word, both
  variants). The layer is then the first game's dialog picture
  (`compose_dialog`, its rows now shared out among the cores): the HD
  view, the body upscaled over it (bilinearly), the HD head where the original's head was
  drawn and the pixels next to it, and with widescreen a character
  touching one edge of the picture moved to the wide picture's edge; the
  dialog's view is drawn once and kept while the camera, the faces and the
  options are the same. The texts (the subtitles @ 0x004611e0, the menus'
  tooltips FUN_0046d750 and save lists, the bag's names, the credits'
  pages): every `TextOutA` call is recorded with its font (`AtlFT_Dialog`
  or `AtlFT_POPUP`), and the 640x480 pixels its glyphs set with what was
  under them (`HdTexts`, the first game's `HdText`); the compositor puts
  back what was under the 640x480 glyphs, and the call is drawn again at
  the window's size (`SubtitleFont::draw_hd`): each glyph at its pen
  position of the original (the `hdmx` advances, the line breaks of the
  original's layout, the `VDMX` ascent), scaled, the outline rasterised at
  16 (or 13) pixels per em times the scale and blended by its coverage.
  The window draws them again exactly where the compositor uncovered the
  640x480 glyphs (`HdCompositor::uncovers_texts`: sharp subtitles, or a
  video zoomed to the widescreen sides), so no option leaves a text
  undrawn (a test composes every option against it). Only `--hd` and
  `--widescreen` reach Atlantis II's `hd_options` for now (no
  `settings.ini` key or menu yet): its other options keep the first
  game's defaults (the bicubic, sharpened panorama, the smoothed art,
  sharp subtitles, black bars beside a video). The
  credits' texts fade with their pages: the old page's at 255 − alpha, the
  new one's at alpha, over the pictures blended without their texts (the
  original blends the pictures with their texts; the two differ by a
  fraction of the fade where both pages have text). A test plays dialog
  360 both ways, with the Pentium II's routines and the C ones (the
  640x480 frames the same, the layer the frame before
  the subtitles, the subtitles the original's calls, the HD head in the
  layer), and the bag's names, a menu tooltip and the credits likewise;
  others check the recorded texts' pixels and, at three sizes, that every
  sharp glyph pixel is within a pixel of the original's glyphs, scaled. A window
  pixel is the 640x480 pixel the composition shows there
  (`window_to_screen`: ⌊(x − ox)·640 / dw⌋, the compositor's nearest map),
  so a click in the menus, the bar or the subject menu lands on the
  original's pixel (a test checks every pixel of six window sizes). The
  window does this with the HD mode off too, where it draws the frame
  with the same nearest map: before, a position was divided by the float
  scale from an offset that could fall on half a pixel, and some clicks
  named the pixel next to the one drawn under them (one pixel away at
  most; a test in `a2.rs` compares the two).
  With the options menu (F10) open over a subtitle, the game's texts are
  drawn again before the menu's, so the menu stays on top. The test
  `hd_widescreen_plays_as_the_original` (`a2.rs`, with the game) plays a
  new game's first view, the shaman's dialog clicked through (the options
  menu over one of its lines), Ireland's and China's first views turned
  around and the main menu with a tooltip at once with HD widescreen in a
  1920x1080 window and with the original graphics: every pass gives the
  same 640x480 frame and the same script events, the HD picture fills the
  sides where the original graphics leave black bars.
  Timings (release, Intel i7-9700K, 8 cores, nothing else running; the
  middle of three headless runs of 300 frames, `--hd` or `--widescreen`,
  `--size <w>x<h> --headless <game> 300` with `--command
  "(monde=3)(changewam=chine.wam)(vue=10)"` (China's view 10), `--view
  7,70,5` (Tibet's view 7) or `--dialog 360` (the shaman's first line),
  which print the pass, the HD view's part of it and the window's
  composition; measured on `a2/m0` with the three HD branches merged):

  | | 1920x1080 | 1920x1080 widescreen | 2560x1440 | 2560x1440 widescreen |
  |---|---|---|---|---|
  | China's view 10: pass (HD view) | 3.7 ms (2.5 ms) | 4.0 ms (2.9 ms) | 4.4 ms (3.2 ms) | 5.1 ms (3.9 ms) |
  | composition | 2.1 ms | 2.2 ms | 3.4 ms | 3.8 ms |
  | frame | 5.7 ms | 6.3 ms | 7.8 ms | 8.9 ms |
  | slowest frame (the view's first) | 32 ms | 33 ms | 41 ms | 46 ms |
  | Tibet's view 7: frame | 6.4 ms | 7.0 ms | | |
  | dialog 360: pass (HD view and dialog composition) | 11.1 ms (4.2 ms) | 11.3 ms (4.4 ms) | 13.7 ms (5.5 ms) | 13.9 ms (5.6 ms) |
  | composition (the sharp subtitles included) | 2.2 ms | 2.4 ms | 3.1 ms | 3.4 ms |
  | frame | 13.3 ms | 13.7 ms | 16.8 ms | 17.3 ms |
  | slowest frame (the line's first) | 45 ms | 51 ms | 58 ms | 63 ms |

  A still view redraws only what its animations change; a turn redraws
  the whole HD view, which `cargo test --release -p atlantis2-engine
  --test hd -- --ignored --nocapture hd_timings` times alone on synthetic
  faces: 4.6 ms at 1440x1080, 6.1 ms at 1944x1080, 7.5 ms at 1920x1440,
  10.1 ms at 2592x1440 (bicubic or bilinear alike), so a turning frame
  takes about 8 ms at 1920x1080 (9.5 ms widescreen). The faces'
  preparation for a new view takes 13 ms (bicubic; 4.8 ms bilinear),
  hence the first frame of a view's 30 to 45 ms; then 0.6 ms when nothing
  changed, 1.8 ms for a sprite's frame. Every case stays under the 16.7
  ms of 60 frames a second at 1920x1080; a dialog at 2560x1440 is just
  over it (about 58 frames a second). Without the HD mode the same runs
  take 1.0 ms a pass in the panorama (3.2 ms in the dialog) and 1.1 ms to
  scale the picture to a 1920x1080 window; the main menu in HD widescreen
  0.7 ms a pass and 3.6 ms to compose. The view is
  computed in blocks of 64 columns by 32 rows, shared out among the cores
  (one on the web); the projection and the face choice take about a third
  of it, the bilinear sampling the rest. A frame's HD pass is printed by
  a headless run with `--size`. The body video and the head's pose of a
  dialog change 15 times a second, the mouth at every frame. The HD head
  alone (`cargo test --release -p atlantis2-engine --test hd_heads --
  --ignored --nocapture hd_head_timings`, `Ais1.db`, the Pentium II's
  routines, its pose included) takes 5.7 ms at 1440x1080 and 8.1 ms at
  1920x1440, against 2.6 ms for the original's draw: the rasteriser is
  the original's, transcribed with its x87 rounding, on one core (its
  faces are drawn in order, the later winning a Z tie).
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
  choice). The game's own Direct3D start-up was measured with the RGB
  software device on Windows 11 (0x027f kept) and Windows 98 SE with
  DirectX 6.1 (0x023f, the same arithmetic): it does not load the single
  precision (0x007f) the DirectX documentation describes. A hardware (HAL)
  device, the one a 3D-accelerated machine of the time would have used,
  was not tried; under it 0x007f may apply (the engine's `Fpu::Single`
  models it). No assembly routine runs before the first view, so a new
  game's first view is entered under the process's word.
- **Turning:** the mouse adds dx / 2000 and −dy / 2000 radians to two
  speeds, which decay × 0.8 a frame; the limits stop the turn (`look.rs`).
  The port turns once per frame, at its 59.94 frames a second (below).
  The arrow keys stand for 8 mickeys a frame each (a port convenience,
  the `ui_ux` enhancements: the original's key callback has no turn key).
  The original reads the mouse once a pass (DirectInput's state, a button
  newly down is a press), so the press that activates its window is a
  click: so is every press here, the pointer free or held, but with
  `ui_ux` the one that gives the window the focus only takes the pointer
  (a press before the next frame: after it the pointer is taken by the
  frame, or cannot be, and a press is a click).
  The window opens in full screen when `atlant2.cfg`'s byte 8 says so (1,
  the original's default: its start-up's mode, both releases), or with
  `--fullscreen`; F11 and 1.00's Alt+Enter switch it and write the byte.
- **Frame rate:** the original's frame task has no period, but in full
  screen (its default) it shows each frame with `Flip(NULL, DDFLIP_WAIT)`
  on a chain of one back buffer (@ 0x0058fdec, the chain made
  @ 0x0058f86c..0x0058f8b2), a flip that waits for the vertical blank:
  one pass per refresh of the screen, at the rate the display mode was
  set to (`SetDisplayMode` with rate 0, the display's default,
  @ 0x0058f81b..0x0058f82a). In a window it copies the frame with an
  asynchronous blit and is not paced. The port runs its passes as on a
  60 Hz screen at the VGA 640 x 480 timing: 800 x 525 pixels at
  25.175 MHz, 16.683 ms a pass, 59.94 a second (`engine::PASS`; the
  window's `FRAME`, the headless mode and the tests). The original on
  another screen ran its per-frame quantities (the turns, the zooms, the
  blinks, the puzzles' steps) at that screen's rate.
- **Time:** as in the original, the frame dispatcher runs once per
  drawn frame (`Engine::pass`: the music's state machine, then the loop
  of the moment: panorama, video or dialog) and a 15 Hz task
  (`Engine::tick`, @ 0x0040caa0) paces the videos, the place animations,
  the cursor, the music fades, the dialog's lines and the script clocks.
  It runs as the original's scheduler runs it (@ 0x00584a00): due when
  the pass's time is past its next time (strictly), at most once a pass,
  its next time then the pass's time + its period, `1.0 / 15.0` kept as
  a float (0.0666666701…, a little over 1/15 s; the sum under the x87
  control word of the moment). There is no catch-up: a late pass delays
  every later tick. So the task's rate depends on the passes': on the
  port's 59.94 passes a second it is due every 4th pass (14.985 Hz); on
  the original the scheduler's golden (`tests/golden/scheduler.txt`,
  the game's own scheduler under emulation) gives every 4th or 5th pass
  on a screen of exactly 60 Hz (the counter's rounding decides), every
  5th at 72.8 Hz, every 5th or 6th at 75 Hz, every 6th at 85 Hz and
  every 34th at 500 passes a second, 12.5 to 15 Hz. In a pass the frame
  runs first and the 15 Hz task after it (the frame task's next time is
  the previous pass's, earlier in the time-ordered queue; the golden's
  frame task runs before the 15 Hz task in every pass):
  `Engine::set_clock` finds whether the task is due and `Engine::pass`
  runs it after the frame, so a click, a film's start or a dialog's
  start handled in a frame sees the ticks of the passes before it, and a
  tick's effects are drawn in the next pass. The window runs a pass at
  the time it was due (as the original's passes keep the screen's pace),
  not at the time its event came. A move's turn is counted in frames, as
  in the original (`engine.rs`). While the window does not have the
  focus the game is held, as the original's 15 Hz task (@ 0x0040caa5)
  and frame dispatcher (@ 0x0049ce25) return at once unless its window
  is active (FUN_0058ff10): no frame runs and the clock stands still (the
  time away does not count, as the scheduler moves the task's last run
  on while it returns). The sound differs, a port choice: the original
  only stops hearing its DirectSound buffers (made without the global
  focus flag), which play on unheard, so a voice, a film's track or the
  music has moved on by the time away when the focus comes back; the
  port's voices stand still while silent and play on from where they
  were (`atlantis-app/src/a2.rs`, as for the options menu).
- **Place animations:** the SPW files of `datas_a2\spw\` are banks of
  frames with up to six sprites each, one per cube face, in the first
  game's sprite rows and blend (`c + dst × (32 − f) / 32`), drawn into the
  512x512 faces before the cube (`crates/atlantis2-data/src/spw.rs`). At
  each view entry every state of every entity shown in that view gets a
  slot (at most 34): first frame, end, click kind, shown or not; the
  show / hide table (a taken object) overrides "shown". Each tick moves
  every shown slot one frame on; at its end a slot restarts (its first
  play one frame shorter, as in the original), stops, or, for a state
  with a weight, draws among the weighted states of its entity, and
  hands over after playing to the end of its file (`anim.rs`); a state
  with a random pause (only `yucatan.wam` view 39's lizard) waits a
  number of ticks drawn at each end. The draws use the game's one `rand`
  (MSVC's; the original's single thread has one state, shared with the
  dialogs, the music, the place sounds and the puzzles): seeded at
  start-up with the clock as the original seeds it with `GetTickCount()`
  (its earlier `srand(time(0))` counts for nothing), again with the 15 Hz
  count at each music start; `--seed N` gives the start-up seed, and the
  tests keep `srand(0)` or their own (`Engine::seed_rand`). The pause and
  the hand-overs are compared with the original's step under emulation
  (`spw_chains.txt`). The faces are drawn on and put back after every
  frame (the original clips each blit to the face's visible rectangle,
  FUN_004daad0; the renderers sample no texel outside those rectangles,
  `tests/centre.rs`, so the pictures are the same). While a character
  speaks in a dialog, the place animation of one of the speaker's
  `[IdText]` names is hidden, and shown again when the speaker changes
  or the dialog ends: the names in their order, and for the first one
  with a shown slot, its lowest (@ 0x0046191d..0x0046195e).
  The horse (`CHEVAL`'s object index) keeps its frame across views as
  the original does: each tick writes its slot's new frame into the
  script variable `FRAMECHEVAL` (@ 0x0047d84d) and a view entry starts a
  new slot of that id at it (@ 0x0047c764); neither tests the slot's
  kind. `ADDFRAMEOBJET=CHEVAL` past the slot's end zeroes the range
  (record +0x20, +0x24) on the loaded WAM's state record itself
  (@ 0x00483565..0x00483574), so it lasts until the next `changewam`:
  view 27's horse plays its whole file at its next entries. A state that is not one animated frame (record +0x2c ≠ 1: only
  Ireland's puzzle pictures in the ported places) draws, instead of its
  current frame, the frames of its 128-byte list (signed bytes, in list
  order up to 0xfe, 0xff skipped; empty at load), which the puzzles' code
  fills: `Anim::add_frame` (@ 0x0047c880: at the end of the list of every
  slot of the id; false with the original's warning when no slot has it
  or the last one's list is full), `Anim::sub_frame` (@ 0x0047c910: in
  the first slot of the id whose list has an end, the last entry equal to
  the frame takes the list's last entry, which becomes the end: the order
  changes) and `Anim::has_frame` (@ 0x0047c9c0: any of the 128 bytes,
  past the end too; @ 0x0047ca50, which returns the frame itself when one
  holds it and −1 otherwise, is the same test, used by the ogham stones).
  Such a slot is hit under the centre only when its list drew a frame
  (the draw @ 0x0047ce60 returns 0 otherwise). Not ported: the clear
  (@ 0x0047ca10, unused so far) and the removal's two writes when the
  list that ends its walk has its end at index 0 (a match index carried
  from a full list of an earlier slot of the id, or frame −2 matching
  that end byte): the byte below the list into the matched entry and the
  end into that byte, @ 0x0047c979..0x0047c988 (no data reaches them).
  State chains are ported: a state whose WAM record names a next state
  (+0x1c, `A1`) keeps that state's record when its slot is loaded
  (FUN_004123b0: the entity's first state with that letter and number),
  so the slot hands over when it ends, and after the view's loads each
  such slot chains to the slot loaded from that record (FUN_0047c820 /
  FUN_0047c850). The only chained states are Chac's animations 1 and 2 in
  Xibalba's view 123 (both to animation 1): animation 1 loops on itself,
  and after `playanimp=CHAC,2` animation 2 hands back to animation 1
  instead of leaving the pond empty. Checked against the original's loads
  and steps run under emulation, with chapter 37's commands on Chac
  (`tests/xibalba.rs`, `golden/spw_chains.txt`). The odd-tick flag
  (+0xc0) is ported with the pearl and dragon puzzle, its only writer
  (the transparent cloud, @ 0x00494bed).
- **Cursor:** the frames of `spr\interfac.spr` (a `SPRCRYO` bank,
  `crates/atlantis2-data/src/spr.rs`: the shade-run state machine of
  the sprite library's `AFF_SPR` and `AFF_SPRTRS50` (the 50 % mode of the
  subjects already asked, the bag bar's and the canoe's pointed tile) and
  their tables for RGB565, the last row and
  column of the screen never drawn; the one bank with a 0x400-byte
  (B, G, R, x) colour table, `Ponpirog.SPR`, converted as the DLL does
  for RGB565; the library is the one the original loads on a Pentium II,
  `spr_p6.dll` (@ 0x0059cb93: CPUID family 6), whose 50 % shade runs are
  `shade(dst, f)` and `half(dst)` where `spr_p5.dll`'s, a Pentium's or
  an AMD K6's, are `shade(dst, f >> 1)` and `half(dst) +
  half(half(dst))`; `Engine::spr_dll` chooses `spr_p5.dll`'s) at the
  screen centre: 1..12 over a move zone, 25..39 over
  another zone (none from type 1000 on), 13..24 or 25..39 over a
  clickable object or character, one frame on per tick, chosen after the
  frame is drawn so it shows on the next one. The object in hand (its
  sprite in the world's objects bank, `spr\objtibet.spr` for Tibet) is
  drawn under the cursor; right after a take the cursor itself shows the
  object for one frame. Then the 5-pixel cross of the "centre marker"
  option (on by default) in the colour of what is under the centre
  (zone type or sprite); its two lines are drawn end points included
  [inferred]. The texel under the centre that the sprite test uses is
  the original's (FUN_0047db60, `Camera::texel`): the centre unprojected,
  turned into the world by the camera's quaternion, turned back by each
  face's quaternion in turn and projected by the face's camera (focal
  254.99998, centre 256), truncated, the first face whose 0..511
  rectangle holds it; every operation in the control word's precision and
  rounding (2520 directions against the original's function under
  emulation, `centre_texel.txt`; the cube mapping it replaced differed by
  a texel in 102 of them). Everything after it (the box test, the cursor,
  the hand, the marker, the take) is checked against the original frame
  by frame.
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
  runs no click scan (below). A type-2 zone moves to another place file
  by its link (@ 0x0049cbf0, @ 0x0040fc60): the first link of the
  place from the current view to the zone's value; with a destination to
  that view the change waits for the move's end (the destination's turn,
  made pending at once when none is needed, the file loaded when the
  move completes, its view entered with an arrival scan, `LASTVUE` = 0,
  the video looked up in the new file), without one the file is loaded
  and the view entered at once (no arrival scan, no video). A
  `changewam` forgets the current view too, so the next `vue=` sets
  `LASTVUE` = 0. The transition sprites (`datas_a2\spf\<prefix><from><to>.spr`,
  one bank per character or object a destination names, unless the
  show / hide table hides it; at most 10) are loaded after the arrival
  and drawn over the video: frame k of each bank at its default position
  over the video's frame k, until the bank runs out; the video's end
  frees them, and a move without a video leaves them for the next film,
  as in the original (@ 0x0047c100, 0x0047c200, 0x0040bc58); an empty
  sprite draws nothing. The transition sprites, the type-2 zone at once
  and two deferred place changes are compared pass by pass with the
  original's frame code under emulation (`links.txt`, `tests/links.rs`). The video
  of a move to another place is the destination old → new looked up in
  the new place's file (@ 0x0045a518): between `chine.wam`,
  `salletortue.wam` and `miniature.wam` there is none (the old view's
  record in the new file has no destination), as in the original. **The
  zoom of a move** (`ZoomStart`, China's two moves into and out of the
  miniature garden's view 10; `turn.rs` `Zoom`): FUN_0045b350 sets the
  step (value × 0.01, a float product) and, when none is pending, the
  field of view to 76° and the target to 35°; the turn's set-up turns
  even under 0.1° while a zoom is pending, and each turn step lowers the
  field of view by the step while above the target, the move waiting
  until the turn and the zoom have both ended; at the target with the
  turn ended, both go back to 76° and the next warp frame skips its draw
  (FUN_0045b390, FUN_0045b680) (@ 0x0045babe..0x0045bb40). Under 0x0e7f
  the step is 0.99999994 and the field of view lands on 35 without that
  reset; under 0x027f it is reset to 76. Checked frame by frame against
  the original under both words (`tests/moves.rs`,
  `scripts_worlds_zoom.txt`). The turn step's comparisons are the x87's:
  a NaN counts as below (`test ah, 1`), which keeps a turn at its end
  while a zoom holds it (the braking divides by 0 there).
  `ZoomPsychoStart=a,b` (chapter 81, every move of the ring) is the same
  zoom (towards b°, a / 100° a frame) with a roll, when none runs (the
  roll's sum and step both 0, FUN_0045b3b0): the step −0.16 rad (the
  float 0xbe23d70a), and while it is not 0 the turn's yaw and pitch are
  not applied (@ 0x0045ba0a..0x0045ba1b; the turn still counts its way
  and ends), the camera turns about its own Z axis by the step after the
  zoom's part, the step summed (@ 0x0045bb45..0x0045bbae); the zoom's end
  undoes the sum and clears both (FUN_0045b400). The view's direction is
  kept, so the move's video and arrival come from where the click aimed.
  Checked frame by frame against the original under both words
  (`scripts_worlds_zoom.txt`'s `ZoomPsychoStart` sections: the field of
  view, the summed roll, the read-back angles).
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
  (@ 0x00495650, a fixed list that is not the fireflies' answer; what its
  "Mandala k" numbers is unknown). The fireflies that read these ranks
  are Shamballa's (below).
- **Skull** (`engine/puzzles.rs`, Ireland's views 40-42 of
  `bigile1.wam`): the view entry of the three views (@ 0x00493cb0, from
  the entry dispatcher @ 0x0049bd50) keeps `CRANE_SPRITE`'s object index,
  takes frames 0..6 out of its slot's list once each when there, then
  adds, for each of the seven characters of the puzzle's state in order,
  the frame c − '0' when it is 0..8 ('9' hides). The state starts as
  `0199999` (frames 0 and 1 shown) and nothing resets it but a load: a
  second new game in the same run keeps it, as the original's does. A
  click on a zone of type 3, 4 or 50 in view 42 (@ 0x00493d40, from the
  click dispatcher, which never takes an Irish click) runs five blocks
  in order: when the zone's value is `CRANE_ZONE`'s index (its name and
  type are not tested) and `OBJETENMAIN` (read again in each block) is
  `CRANE_CROIX`, `CRANE_RUCHE`, `CRANE_PUIT`, `CRANE_CHAPELLE` or
  `CRANE_W18`: frame 2, 3, 4, 5 or 6 added to the list of the index the
  last entry kept, the cursor pair cleared, that digit written at that
  position of the state, `RunOneCommand("(NewSound=7)")`. No variable is
  written and the object stays in hand: chapter 7's click scan of the
  same frame counts `fincrane`, empties the hand, takes the object from
  the bag and plays sound 3 (which stops sound 7 on voice 2 in the same
  frame, as in the original); the fifth object plays film 2, then shows
  Ailill and hides the skull. No guard: the same object twice is listed
  twice. The state is save entry 30. Against the original's emulated run
  (`tests/skull.rs`, `tests/golden/skull.txt`): the frame-list functions
  on three slots, a full list and no slot; the entry per state string
  (lists given before, no slot, two slots); the click per object, wrong
  objects, an empty hand, zone variants, twice, all five, before any
  entry; the dispatchers' choice by warp name, view and zone type; the
  click and the scan of chapter 7 for three orders of the five objects,
  line by line, with the bag, the variables, the list, the state and the
  two slots shown; the drawing of the lists of the three sprite files
  into six faces (md5 of each face, pixels, the slot hit with the centre
  on a listed frame). `the_skull_played_with_the_mouse` plays it from
  chapter 3's view 28: the walk to views 40, 41, 42 (chapter 7), each
  object picked from the bag bar and laid with a click, film 2, then
  Ailill clicked: dialog 31. Not known: whether sound 7 is heard at all
  on the original (it is replaced by sound 3 in the frame it starts; to
  settle by listening to the original in the Windows 98 machine) and the
  box a listed frame equal to the file's frame count would take (read
  past the frame table; only a crafted save holds such a digit; to settle
  by emulating FUN_0047cb30 on such a frame).
- **Ireland** (world 1, disc 1; `bigile1.wam`, `livre.wam`,
  `ilepoete.wam`, `ilepoete2.wam`, `puit.wam`; chapters 2-9, 11-17, 19,
  20 and 24): laying Ireland's boat in the shaman's hut runs chapter 75's
  section as the original does: the world (disc 1, `objeire.spr`,
  `sujeire.spr`, the Irish dialog texts and heads), the departure film,
  `bigile1.wam` view 5 and chapter 2, whose `/INIT` plays the first
  arrival's dialog. The chapters' sections then run on the same engine
  as Tibet's (every key they use is handled, `tests/script.rs` checks
  each of their 653 `/set` lines against the original's handler), and
  chapter 16 sends the game back to Tibet's view 7 with Ireland's mandala
  in the bag once the story sets its flag. **Brigid's cross** (view 2,
  chapters 19 and 16, `engine/cross.rs`) is ported as the original
  plays it: Liam gives the cross in chapter 19; laid on one of the
  view's 25 places (action zones of type 4, values 1..25) it is drawn
  there (frame value − 1 of `CROIXBRIGIT_SPRITE`'s frame table), leaves
  the hand and the bag, and sounds 8 then 9 play; a click on the place
  that holds it takes it back (hand, bag, sound 9); any other click on a
  place plays sound 9. On the place of value 1, `FINCROIX` = 1 and a
  scan: chapter 16's section plays the goddess's film, then her dialog
  56. Kept as the original: the state is the place's value (@ 0x006d26b4,
  save entry 32), which no instruction but the hook's writes, so the port
  resets it nowhere, not at a new game of the same run either [whether
  the original's new game clears it through some block clear is not
  settled: no direct reference does; emulating the menu's new-game path
  (state 10 of FUN_00486eb0) with the state set, or laying the cross,
  starting a new game on Windows 98 and looking at view 2, would settle
  it]; the view entry draws the cross from it;
  the hook has no guard (laid again while laid elsewhere, by a script
  giving it again, the older picture stays for good); the two sounds of
  a placing play on voice 2 after the scan, so at the win sound 8 stops
  the film's track and sound 9 stops sound 8: the goddess's film plays
  with sound 9 instead of its own track [read in the code and in the
  order of the emulated calls; not yet heard in a run of the original:
  listening to the win on Windows 98 would settle it]. Checked against the original
  (`tests/cross.rs`, the hooks, the zone click with its scans, the
  drawing of every frame, a played game from chapter 19). The skull
  (views 40-42), the ogham stones (view 6, with `COUTEAU_AILILL` in
  hand) and the blackbird songs (view 73 of `ilepoete.wam`) are ported
  too (below); each hook that runs is noted (`Event::PuzzleRan`, the
  original's function). The original's dispatcher never takes an Irish
  click (@ 0x0049baba, @ 0x0049bcf8): the click goes on to `CLICZONE`,
  the click scan and the move as in the original. The zones of views 2
  and 6 are of type 4, whose values (1..25 for the cross, 1..18 and 99
  for the stones) `CLICZONE` takes as they are (@ 0x0049cb9b; the load
  fix-up FUN_00410020 turns only type-3 values into indices of the named
  click zones), and no Irish section names a view: so in the original a
  zone of value N there answers for the named zone of index N, in both
  views: 1 `CRANE_ZONE` (chapter 7, a skull in hand: laid), 4
  `TETE_PLEURANTE` (chapter 12: empty-handed, back to Tibet; the jug:
  filled), 5 `MENHIR` (chapter 8, the ogham staff: film 5), 6 `NUADA`
  and 7 `FILLE_LIVRE` (chapter 5, the pencil, with their flags), 8
  `HAUT_CROIX` (chapter 9, the fork), 9 `EAU` (chapter 14, the horse:
  film 6), 10 `PUIT_ZONE` (chapter 20, while dowsing). With the minor
  bug fixes ([Enhancements](#enhancements)) a click on those zones
  leaves `CLICZONE` as it was, so it runs its puzzle hook and nothing
  else (`irish_plain_value_zone`; `tests/plain_values.rs`: every type-4
  zone of both views in every Irish chapter, with an empty hand and each
  object an Irish `cliczone` condition names, both ways). The move-zone hooks of
  views 2, 6 and 42 clear the cursor pair (FUN_0047bef0(0, 0), whatever
  is in hand) until the next cursor choice; view 73's is the blackbird
  game's leave (below). A headless game plays the whole of Ireland with
  the mouse (`tests/flow.rs` `a_game_from_tibet_to_ireland_and_back`:
  the view turned by mouse moves, every action a click, every object
  picked from the bag bar, from the arrival to the way back), each
  puzzle solved as a player does (the skulls found and laid, the ogham
  stones by the original's shortest path, the horse caught as it runs
  past, the blackbird songs by ear, the cross laid, taken back and laid
  on its place), and every chapter it goes through (2-5, 7-9, 11-16, 19,
  20, 24) is left by one of its own `chapter=` lines; chapters 6 and 17
  hold sections no line leads to. `demo.wam` (chapter 1) is not
  ported: no script reaches chapter 1 and its panoramas are on no disc.
  One place animation of `bigile1.wam` (`woA1001001.SPW`, a shadow in
  view 1, hidden at load) is on no disc; it is skipped with the
  original's message. The place sounds of `bigile1.wam` play (Sound,
  below).
- **China** (world 3, disc 3; `chine.wam`, `miniature.wam` loaded by the
  scripts, `salletortue.wam`, `labysol.wam`, `labyplaf.wam` reached by
  their links; chapters 42-48, 55-60): laying China's boat in the
  shaman's hut runs chapter 75's section as the original does (the world
  on disc 3, `objchine.spr`, `sujchine.spr`, the Chinese texts and heads,
  film 21, `chine.wam` view 10, chapter 42 and its music); the gong taken
  in view 6 or 7 and struck on its zone in view 5 (chapter 42), or the
  lantern brought back (chapter 46: films 32 and 33, `chine.wam` view
  200, dialogs 346 and 347, China's mandala in the bag, the hand empty
  at the arrival as in the original), sends the game back to Tibet's
  view 7 and chapter 75. Every key of the Chinese chapters is handled; their
  1035 `/set` lines (with Yucatán's 820 before them, the state carried
  from chapter 1 as the original's run did) match the original's handler
  (`tests/script.rs`, `scripts_worlds_china.txt`). New for China:
  the zoom of a move (above); **the picture viewer** (`ViewBigSpr`, loop
  type 11, `LoopType::Picture`): FUN_0048c9d0 loads `spr\<bank>` (the
  name up to `)`, a CR or 63 characters; a bank still loaded is freed
  with the original's warning) and the section goes on; each frame
  FUN_0048ca50 draws the panorama with its place animations and their
  step (FUN_0045a930), frame 0 of the bank with its hot spot at the
  screen centre, and a click goes back to the panorama (loop type 0, not
  the previous one), the bank freed, the turn speeds stopped; Escape
  opens the main menu as from the panorama (the key callback's default);
  **`NextWarpInvalide`** = 1: the next view entry is skipped (@ 0x0045a01a:
  the view current, no panorama, camera, animations nor sounds; the
  variable 2), and the warp frames draw nothing while it is 2 (the
  screen frozen) until a `vue=` or a move to another place clears it, as
  chapter 59's door uses it over `miniature.wam` view 122, whose
  panorama is on no disc; **the special events** (`TestSpecialEvent`,
  FUN_00484880, `script/frame.rs`): events 1-6 are the labyrinth's six
  demons (view and frame each), events 7 and 8 Yucatán's `CHAC`; checked
  case by case against the original (`scripts_worlds_events.txt`) and
  played in chapter 60 (the rat given a form: sound 203 at frame 51 when
  voice 2 is idle then, the event's end at frame 60, the stamped form
  back, the remembered dialog; the test mixes each tick's sound so the
  voices end, and runs it with the view's souls, whose passage sound 220
  holds voice 2 at frame 51, and without them, when 203 is heard; the
  mouse-played chapter 60 checks the same rule at each demon's stamp).
  **The five wheels and the bridge** (`SS` views 12-13, chapter 55,
  `engine/wheels.rs`) are ported as the original plays them: the five
  discs `DISQUE1..5` (the figurines' gifts, chapter 48) are laid on the
  axle's five places (the zones `AXE_5ROUES1..5` of view 13); each disc
  has its own wheel sprite `RoueK<k>` (one frame per place) and its
  variable `pos_roue<k>` (the place, 0 when not laid). The view entry of
  views 12 and 13 (FUN_0048d080) keeps the five wheels' records (the
  first record of each id, any kind; none is the original's fatal) and
  places them from the variables: hidden at 0, otherwise held still at
  frame place − 1. The click (FUN_0048d170, on zone types 3, 4 and 50,
  never taking the click) does nothing once `findisque2` is 1 or on a
  zone whose name does not start with `AXE_5ROUES` (a case-sensitive
  `strncmp`); the place is the name's 11th byte − '1'. Every wheel shown
  there is taken off (its variable 0), then the disc in hand is laid
  (its wheel at that frame, the disc out of the hand and the bag, its
  variable the place) and the check runs; the disc taken goes to the bag
  and the hand: a disc laid on a taken place swaps. The check
  (FUN_0048d310): the order 1 2 3 4 5 with `findisque1` 0 hides
  `Pont_Leve`, shows `Pont_Moitie`, sets `findisque1` and plays film 24;
  then 5 3 1 4 2 with `findisque2` 0 hides `Pont_Moitie`, shows
  `Pont_Baisse`, sets `findisque2` and plays film 25; chapter 55 then
  lets the move from view 12 to 14 go on to chapter 56 instead of
  aborting it. The wheels need the place animations' playing byte
  (record +0xbc, set at load): the 15 Hz step moves a slot's frame only
  when it is set (`Slot::playing`); the wheels clear it, and so does
  `ANIM_THE_FRAME` 0, now ported on it (no chapter uses the key; a value
  other than 0 or 1 is the original's `SetVariables(): ANIM_THE_FRAME !=
  0 ou 1`, the byte written after it). Kept
  from the original: two discs on one place (only a script could put
  them there) are both taken and only the last reaches the hand and the
  bag. Checked against the original (`tests/wheels.rs`,
  `tests/golden/wheels.txt`: the entry, the click on every kind of zone
  name, the check with its film's music and sound track, the engine's
  dispatcher on each golden row, `ANIM_THE_FRAME` on the held wheels,
  the zone clicks with chapter 55's
  scans, every wheel frame drawn, a game played with the mouse from
  chapter 48 to chapter 56 across the bridge, the wheels drawn again
  after a load). The first click outside views 1-7 while the chest's
  puzzle mode (5) is set only clears the mode (@ 0x0049b96d, the
  dispatcher returns before any hook, golden row `D ss0013.hnm 13 3 mode
  5`), as the chest's port does.
  The figurines on the floor and the chest (`SS` views 1-7), the pearl
  and the dragon (`SS` 31), the turtle ring (`CH` 40-64) and the
  labyrinth's souls (`LA` 1-60) are ported too (below): every Chinese
  hook the original's dispatchers call (`chinese_click_hooks`,
  `chinese_entry_hooks`) runs, and none takes a click (the dispatcher
  returns 0 for all of them), so every click reaches `CLICZONE`, the
  scans and the moves as in the original. The testing aid that wrote
  the puzzles' wins is gone: every win is played. The turtle ring and
  the souls write no variable (the turtle
  room's way out is the scripts' count of arrivals, the labyrinth's the
  scripts' form and fan). The incense sticks are the scripts' alone
  (`GOTOFRAMEOBJET`, `showobjet`) and play as written. Four view records
  stand only for a link's other end and have no panorama on any disc
  (`miniature.wam` 47 and 122, `salletortue.wam` 32, `labysol.wam` 123);
  one place animation (`1mA1024025.SPW`, renamed `.err.SPW` on disc 3)
  is skipped with the original's message; music 21, which chapter 75's
  trip copies, is on no disc and no list plays it. The place sounds of
  `chine.wam` and `miniature.wam` play. The space travel from China
  (chapter 56, `miniature.wam` view 16) leads to the hub and back
  (below). A headless game plays the whole of China with the
  mouse only (`tests/flow.rs` `the_whole_of_china_with_the_mouse`, about
  50 s): from a new game, China's boat laid in the hut, then chapters
  42-60 stage by stage (the shadow, the abbot and the gardener master's
  ritual, the turtle room: the ring turned, the turtle laid, the master's
  eight-move walk to the dragon gate; the chest's chopstick and the
  incense; the figurines taken in the grass and each tried on the floor
  until one holds; the coins for the discs; the five wheels in their two
  orders; the arrow turned to the hunter; the pearl won in five rounds
  as it hides in the order ABBAB; the door passed three times, the
  immortal; the labyrinth's forms stamped in the fan's order, the
  mushroom; the lantern), a save and a load through the menus after the
  wheels, and the way back with the lantern (the pearl taken back from
  the mechanism, film 27, down the garden to the turtle room's link and
  view 40's link: films 32 and 33, Tibet's view 7, China's mandala).
  Every chapter it goes through is left by one of its own `chapter=`
  lines, every Chinese puzzle hook runs, none is noted, and every
  Chinese `/set` line it reports is one the original runs. A shorter one
  (`a_game_from_tibet_to_china_and_back`) asks the gardener master and
  the worried monk all they offer and goes back by the gong. With
  `ATLANTIS2_PNG_DIR` set both write their screens (`china-*.png`, the
  puzzles' `pz-*-flow-*.png`). And
  `tests/china.rs` runs every Chinese chapter's `/INIT`, enters every
  view of the five places with its hooks, moves along every link,
  plays the lantern's way back from the turtle room (chapter 46's
  section from the move, its `/set` lines all the original's), plays
  all 115 dialogs the chapters start, checks the subjects, objects and
  musics on disc 3, and loads a game saved in the turtle room.
- **Yucatán** (world 2, disc 2; `yucatan.wam` loaded by chapter 75's
  trips, `salleroi.wam` (the king's hall) and `pyratez.wam` (the
  pyramid) reached by the links of `yucatan.wam` views 39 and 34;
  chapters 25-34): laying Yucatán's boat in the shaman's hut runs
  chapter 75's section (the world on disc 2, `objyuc.spr`, `sujyuc.spr`,
  the Yucatán texts and heads, film 40, `yucatan.wam` view 10, chapter 25
  and its music); laying it again later (chapter 75's second-trip
  section) goes back. Every key of chapters 25-34 is handled (their `/set`
  lines match the original's handler, `tests/script.rs`). New for
  Yucatán: **`AngleWarpXNow` and `AngleWarpYNow`** as the original turns
  the view, read and emulated (`tests/golden/turn_now.txt`, both control
  words, the camera's quaternion and the angles read back bit for bit):
  `AngleWarpXNow` (FUN_0045ad40) **sets** the camera's rotation about the
  vertical axis relative to the scene root, dropping the pitch (the
  angles read back are 90° − the angle and 0), `AngleWarpYNow`
  (FUN_0045ada0) pitches the camera from its current orientation (both
  data uses are 0). The port's earlier guess (the yaw set, the pitch kept)
  is replaced; chapter 26's arrival in the king's hall (270), chapter
  32's in view 63 (360) and chapter 33's in view 67 (180) use them. **The
  Tezcatlipoca statue** (FUN_0049b910's `YU` view 122 branch,
  @ 0x0049bb62..0x0049bbbc, `engine/yucatan.rs`): a click on the zone
  `ClicTezStatue` while `TEZ_STATUE` is not hidden (FUN_00484d90, the show
  / hide table, as for the transition sprites) makes the statue the
  clicked character (`CLICPERSO`), sets the cursor pair to
  `interfac.spr` frame 40 and opens its subject menu, whatever is in hand
  and whatever its subject count; `CLICZONE` is set as for any action
  zone and no click scan follows (the menu is open); the menu then works
  as a character's. The statue stands in Xibalba's `xibalba.wam` (chapter
  39, below): `tests/yucatan.rs` loads the place to play it alone (the
  click, the menu, a subject's answer, dialog 193; hidden, nothing), and
  `tests/flow.rs` reaches it as a player does. **Yucatán's other hooks** (the `YU`
  branches of both dispatchers, `yucatan_click_hooks`,
  `yucatan_entry_hooks`): the Maya counting grids of `pyratez.wam` views
  55, 64 and 75 (their entries, their clicks on zones of type 50, their
  leaves on move zones) run (below); none takes the click, as in the
  original, so every click reaches `CLICZONE`, the scans and the moves.
  Xibalba's hooks under the same prefix all run (below): the star
  map's (view 121), the canoe's (view 402), the canoe's view taking the
  click while `enable_pz_pirog` is not 1, as in the original (the
  dispatcher's own return of 1), and the spider web's (view 198, and
  every view in its puzzle mode 1), the dispatcher returning the web's
  move click's value (1 takes the click). Yucatán's end (chapter 28's
  arrival in the king's view 86 with the soul `CHULEL`: film 55 with
  dialog 168 over it, Yucatán's mandala, Tibet's view 7) needs the soul,
  which only Xibalba's chapter 39 gives (below). **The way back to Tibet
  without it** is chapter 30's brazier (`yucatan.wam` view 35, a click
  with an empty hand: Tibet's view 7, chapter 75); the space travel of
  view 22 (chapter 25) leads to the hub and back (below). Every
  panorama of the three places is on disc 2; one place animation
  (`w3A1078078.SPW`, the closed jaw of view 78) is on no disc and is
  skipped with the original's message; the place sounds of `yucatan.wam`
  play. `tests/yucatan.rs` runs every Yucatán chapter's `/INIT`, enters
  every view of the three places with its hooks, moves along every link,
  plays chapter 26's turned arrival, the brazier, the end with the soul
  put in the bag, all 70 dialogs the chapters start, checks the
  subjects, objects and musics on disc 2, the place sounds, and loads a
  game saved in the pyramid. The game with the mouse is described with
  Xibalba's below.
- **Xibalba** (world 6, disc 4, chapters 35-41; the views keep Yucatán's
  `YU` prefix, the objects, subjects and texts are Yucatán's): `bat.wam`
  (the bat's island, chapter 35), `riviere.wam` (the river, 36),
  `xibalba.wam` (Chac's pond, 37; the statues' clearing, 39) and
  `toile.wam` (the spider's web, 38); 40 and 41 hold only an empty
  `/INIT`. The way in is the priestess's yes (chapter 27, once she has
  the bloody bands and is asked about Tezcatlipoca: the prince and the
  high priest hidden, world 6 on disc 4, film 43, `bat.wam` view 403,
  chapter 35, whose `/INIT` sets `flag81` and plays dialog 171), and
  later the bat of her view (film 44); the way out, the zone `BackToS27`
  of view 403 (film 45, world 2, the king's hall's view 85, chapter 27).
  `riviere.wam` and `toile.wam` are reached by the links of `bat.wam`
  view 404, `xibalba.wam` views 117 and 124 and `riviere.wam` views 300,
  301, 311 and 314; chapter 35's `nextvue=404` (once the canoe is won)
  sends the move from view 403 to 504 on to 404. Every key of chapters
  35-41 was already handled (their `/set` lines match the original's
  handler, `tests/script.rs`); new here are only their places: Chac's
  special events 7 and 8 (view 123, his animation 2 to its frame 60:
  dialogs 185, 186), `ChangeAnimGotoPerso=CHAC,1,1`, the zones of the
  statue's view enabled and disabled by name, `ShowBarque` (stored,
  never read), `LIMITEANGLEWARPY` in view 121. Every panorama and place
  animation of the four places is on disc 4, as are the musics 13, 14 and
  16 and films 43-52; the place sounds of `xibalba.wam` and
  `riviere.wam` play (`bat.wam` and `toile.wam` have none). A save in
  Xibalba loads (chapter 39 runs its `/INIT` again at a load, as in the
  original); a save made in the web (views 198-265) loads in view 198, as
  the original's load sets it (@ 0x0048f450..0x0048f494), with the
  stage's variables from the prey in the bag (the web's load hook,
  below). **The star map** (`engine/stars.rs`, `xibalba.wam`
  view 121, chapter 39) is ported: the seven stars `ETOILE102..115`,
  taken in the clearing's views 102-115, are laid on the 17 holes of a
  stone, the view's zones of type 50 (values 0..16); each hole is a
  frame of `Etoiles_Puz`'s frame list and a variable `etat_etoile<hole>`
  (the star there, 0 when empty: the save holds nothing else). The
  entry (@ 0x00494e00) keeps the object's id, holds its first slot at
  frame 0, shown (+0xbc = 0, +0x10 = 0, +0x30 = 1), empties the id's
  frame lists (FUN_0047ca10, `Anim::clear_frames`) and adds frame h for
  each hole h whose variable is not 0. A click on a hole (@ 0x00494e80;
  nothing once `etoiles_fini` is 1) lays a star in hand on an empty hole
  (any object between `ETOILE102` and `ETOILE115` by index: frame h
  added, `etat_etoile<h>` = the star, the star out of the hand and the
  bag, `(pz_etoile_started=1)`, `(NewSound=115)`) or takes back the star
  of a full hole with the empty hand (frame taken out, 0, the star into
  the bag and the hand pair, not the cursor pair: `(NewSound=114)`),
  then always runs the win test (@ 0x00494f60): the holes 0, 2, 6, 8,
  11, 13, 16 all not 0 (any star in any right hole) set
  `Etoiles_JustFinished` and run a scan, whose film 49 drops the click
  scan of that frame; chapter 39 shows the statue and enables its zones.
  `pz_etoile_started` is read by chapter 35's `/INIT`: the bat offers its
  subject about the stars (dialog 177 / 178) once a star was laid even
  with none left in the bag. `tests/stars.rs` compares the original's
  entry, click, win test, dispatchers, the clicks with chapter 39's
  scans and the drawing of the map's 17 frames (`tests/golden/stars.txt`;
  after a fatal error the original's game quits, so its rows past one
  are compared up to it), and plays the map with the mouse from chapter
  39: the stars taken, a wrong lay taken back, all seven on wrong holes,
  a save loaded, the bat's subject in chapter 35 (dialog 177), the stars
  moved to the right holes (the win: lines 27-33, film 49), the HD view
  of the last hole before and after its star (where the 640x480 frame
  changed, the HD view shows the new pixels), the statue asked through
  its zone; `tests/hd.rs` aims at each right hole of the map entered with
  and without the seven stars: the map's frame list is the seven holes,
  and where the frames differ the HD view shows the star. Which constellation the right holes
  draw, and where the game hints at it, was not looked for. **The canoe runs** (`bat.wam`
  view 402: its click taken while `enable_pz_pirog` is not 1, then
  FUN_0049c010(1), the 3D canoe's own loop type 9, below, and the click
  goes on). **The spider's web runs** (below). No testing aid is
  left: every win is played.
  **A headless game plays Yucatán and Xibalba with the mouse only**
  (`tests/flow.rs` `yucatan_with_the_mouse`, about 90 s in release):
  from a new game, Yucatán's boat taken in view 5 and laid in the hut,
  then chapters 25-34 stage by stage (the prince and the high priest
  asked in the king's hall, the priestess and her clean bands, the king,
  his daughter and the counsellor asked, the priestess again, the king
  given the bands (film 41), the hall asked again, the bloody bands, the
  hall in mourning, the bloody bands back to the priestess (film 42), her
  question declined twice and her other subjects; the obsidian of view
  40; the pyramid's first guard and his riddle, the way on barred, the
  walk back to the king's hall for the Maya numbers' subject, the first
  grid played with a wrong try (three dots) before its answer (a bar and
  four dots), the guard's pass; the second guard's riddle, the second
  grid laid with 81 and the third with 6561 (the riddles' numbers written
  in Maya digits from the units' group up), the entrance door barred,
  opened, closed and opened by its buttons, the crocodile's foot, which
  closes the door behind, the two mirrors and the secret door back to
  view 28, the foot and the obsidian taken back), a save and a load
  through the menus, the way back to Tibet by the brazier and the second
  trip; then the priestess's yes and Xibalba (the bat asked about
  everything, the canoe's zone clicked and the canoe played with the
  mouse to its win (each tile taken to its square and turned upright,
  the stage clicked, the river paddled), the web's first round played
  (the caterpillar), the caterpillar given to Chac (event 7), the
  priestess visited by `BackToS27` and the way back by the bat of her
  view, the seven stars taken and laid on the star map with the mouse
  (one on a wrong hole first and taken back), the
  statue asked before and after the obsidian is laid on the small
  statue, the crocodile's foot laid on its own (films 50, 51: the
  serpent), the serpent asked (the feather), the web's second round (the
  grasshopper, the second spider) and the grasshopper given to Chac
  (event 8), the third round (the feather laid, the sword, its blow on
  the big spider: film 48, the skull),
  the skull given to the serpent (film 52: the soul)), a save and a load
  in Xibalba through the menus, the way back with the soul (the bat,
  `BackToS27`, the priestess, the king: film 55, Yucatán's mandala,
  Tibet's view 7) and Yucatán's mandala laid on its zone, the game
  left in Tibet with the three puzzles' end variables set and every
  puzzle function of Xibalba a player reaches seen to run. Every
  character is asked about everything he offers at each step; the
  dialogs of chapters 25-41 it plays are 101-123, 125, 129, 132, 134-136,
  138-143, 145-167, 169-176, 184-186, 190-202, 204 and 210 (168 over film
  55; the others belong to a click on a spider, which the rounds avoid,
  187-189, to the canoe started and not won, 208, or to the priestess's
  and the bat's answers at other stages; 144 does not exist). The web's
  rounds are those its golden plays (`tests/golden/web.txt` section F),
  each move clicked once the spiders stand, the screen's centre off them.
  Every chapter it goes through is left by one of its own `chapter=`
  lines, no world is refused, no puzzle hook is noted (each
  runs: `Event::PuzzleRan`), and every `/set` line of
  chapters 25-41 it reports is one the original runs. With
  `ATLANTIS2_PNG_DIR` set it writes its screens (`yucatan-*.png`,
  `xibalba-*.png`, `pz-web-*.png`). `tests/xibalba.rs` also
  runs every Xibalba chapter's `/INIT`, enters every view of the four
  places with its hooks, plays the click hooks (the canoe's view taking
  the click, then the canoe started; a star laid on the map), moves
  along every link (the web's only with a transition video), plays the
  canoe with the mouse to its first and second wins and quits one with
  Escape, checks it at 800 x 600, plays the web's first round with the
  engine's clicks, plays all 34 dialogs the
  chapters and the web start (171-202, 204, 208), checks the subjects,
  objects, musics and films on disc 4 and the place sounds, and loads a
  game saved in the statues' clearing and one saved in the web.
  `tests/hd.rs` checks Xibalba's four places, and the star map with its
  seven stars drawn, with the HD mode as the others (the 640x480 frames identical pass by pass, the HD view's
  colours the panorama's). Not checked against the original running
  Xibalba (Wine / Win98): the pictures, the place sounds' mix, Chac's
  events' timing on screen.
- **The space-travel hub** (`planetes.wam`, chapter 83, prefix `PL`;
  its panoramas, videos, music 39 and films 200 and 201 are on every
  disc): the world is kept (no `monde` line; the objects bank and the
  texts stay the traveller's). The shaman's balls (chapter 75's subject
  `SHAMAN`: `BOULE_SHAMAN` and one per world, moved to worlds 1-3) open
  the ball overlay; a click on the zone the ball names (Tibet's view 4,
  `ilepoete.wam` / `ilepoete2.wam` view 70 / 170, `yucatan.wam` view 22,
  `miniature.wam` view 16) sets `SpaceTravel`, whose sections in chapters
  20, 25, 56 and 75 play film 200, load the hub (view 1) and its
  chapter, with `SourceWorld_SpaceTravel` and, the first time from that
  world, `TravelXXDone` and one more `SpaceDeep`. Chapter 83's `/INIT`
  disables the move from view `SpaceDeep` to the next (1..4) and enables
  those before it; 0 is the scripts' trap (a fatal error, as the
  original's). Each move's departure section names its sound
  (`TrajSound`), played with the video. In view 4 the four balls become
  the four crystals, once. View 1's zone 10 is the way back: film 201,
  the source world's place, view and chapter (Ireland's second island
  once `flag70` is set). View 5's last zone (value 77450, no view) is the
  end: `monde=4`, the music stopped, films 68 and 66, the credits
  (`credits=1`: the menu loop's credits page, a resume point left), then
  film 99 and `exit=1` (the game quits). A click or Escape in the credits
  quits at once (the menu's key action), without film 99, as the
  original. Two view records have no panorama on any disc: 10 (the way
  back's destination, which the departure section leaves before the
  move) and 11 (a record no zone leads to).
- **Shamballa and Atlantis** (world 4, disc 4; chapters 78, 80-82 and
  201; `voiles.wam` loaded by chapter 75's crystal (its section needs the
  six `pose*` flags and `flag261` = 0; with `flag261` = 1 the crystal is
  the hub's last trip), `rhea.wam`, `annulair.wam`, `atlantis.wam` loaded
  by chapters 78, 80 and 81, and reached by the links of `rhea.wam` view 3
  and `annulair.wam` view 5): `voiles.wam`'s six move zones of view 1 are
  the fireflies (each departure shows its firefly and starts the zoom;
  the zones lead to views 1..6, of which only view 1 is in the place: a
  move's end without a view runs FUN_0045a630, which in `voiles.wam` sets
  the field of view back to 76° and, the puzzle won, sets
  `Puzzle_Lucioles_AutoWin` and scans: film 62, `rhea.wam`). The vortex
  (`TOURBILLON`, click kind 3) leads back to Tibet's view 10 before the
  win, to Rhea after. Rhea (view 6) answers seven subjects; the shadow's
  (`OMBRE_SHAMB`) opens the link of view 3 (film 67, the ring). The ring
  (`annulair.wam`, 16 views round, `ZoomPsychoStart` on every move): its
  sixteen objects (`*_LA`) are taken by a click, nine characters answer
  a click with a dialog (399-407); `ETOILE_LA` in hand on view 5's stone
  (`ZONEPOSEETOILEPIERRE`, a link of type 2) is the way to Atlantis (film
  63; the sixteen objects leave the bag); without it the departure is
  aborted. Atlantis (chapter 82, `EnergieJoueur` = 10): views 1-18, the
  sunken crystal (`CRISTAL_ENGLOUTI`) in view 18. Its ends are the octopus
  fight's: the hero's death (chapter 201: film 64, view 8 and chapter 82
  again) and the octopus's (chapter 200: film 65, Tibet's view 7, chapter
  75, `flag260`, whose section plays dialog 372, counts a trip and sets
  `flag261`). Three of Atlantis's view records have no panorama on any
  disc: 802 and 810 (they hold the octopuses' and the plankton's
  animations, which the fight loads) and 19 (a destination of view 18
  that no zone leads to). Of the transition videos of the four places
  only `rh001002.hnm` is on a disc: the other moves are cuts, as in the
  original (whose video start fails the same way).
  **The fireflies** (`engine/shamballa.rs`, `Engine::fireflies_click`,
  catalogue row 17): the dispatcher's `SH` branch (`shamballa_click_hooks`:
  `SH` view 1, any case, zone types 1 and 2) runs FUN_00495210, which
  does nothing for a type 2 and, for a type 1 of value v in 1..6, adds 1
  to `NbLucioleAllumed` and stops the turn's speeds; then, whatever v, it
  compares the count with `OrdreMandala<t[v]>` (t the byte table
  `FIREFLY_MANDALA` @ 0x005e1d38: 1:4, 2:1, 3:2, 4:5, 5:3, 6:6, read
  first the count, then the rank) and, equal at the count 6, sets
  `Puzzle_Lucioles_Fini`; then it runs `(NewSound=300)`, whose
  `sound300.apc` is on no disc (it plays nothing; `Event::FireflySound`
  notes it). Only the sixth click is tested: the firefly of the mandala
  laid last in Tibet wins, whatever the five before; the count is reset
  by chapter 78 only (its `/INIT` and the vortex's section), so after six
  wrong ones the vortex back to Tibet and the crystal laid again are the
  way to try again. A value outside 1..6 (no zone has one) warns and
  ends on `DefName_GetIdName`'s fatal error, the name read around the
  table not being a variable (`OrdreMandala%d` of the signed byte @
  0x005e1d38 + v: `OrdreMandala0` for 0 and 7, `OrdreMandala40` for 8).
  The port has the executable's bytes 0x005e1d28..0x005e1dd8 (v in
  -16..160); for a v beyond them, the byte the original reads (another
  string, the `.data` it writes at run time, or an unmapped address) is
  unknown to the port, whose message names the byte's address instead. The click is not consumed: the move goes
  on; the win's move end (a firefly other than view 1's) runs the
  automatic way on (film 62, Rhea), a win on view 1's own firefly waits
  for the vortex. The puzzle draws nothing of its own (each lit firefly
  is chapter 78's `showObjet`). `tests/fireflies.rs` compares the click
  alone (every type, value, count, the turn's speeds), whole games (the
  720 orders for six placements), the move end under both control words,
  the place test, the dispatcher, and the clicks played in the place
  with chapter 78's lines, with the original's
  (`tests/golden/fireflies.txt`); `tests/flow.rs`
  `shamballa_s_fireflies_with_the_mouse` plays a wrong order, the vortex
  back to Tibet, the crystal again and the win with the mouse. Not
  checked against the original running (Wine / Win98): the pictures of
  the lit fireflies.
  **The octopus fight** (`engine/octopus.rs`, the original's module
  `pz_poulpe`, puzzle mode 6) runs: the entry in every `AT` view 1-19
  (the crosshair `viseur.spr`, the camera's reset, the records), the
  tick once per warp frame (the view's yaw in four sectors, the
  countdowns, the strikes at the tentacles' frame 44, their blows at
  frame 62 with the shake and `EnergieJoueur`, the shots with
  `CRISTAL_ENGLOUTI` in the bag, `BigTenta<k>Died`, the two `chapter=`
  ends), the cursor in the warp draw instead of the centre marker
  (`CentreDraw::Crosshair`), the quit (also in a load's reset). As in
  the original: only view 16's big octopus is ever loaded (the module's
  table enables no other: the small octopuses of the other views, their
  death and `PoulpeWarp<v>`, are dead code, and their files are on no
  disc); every `AT` view entry and the quit turn the view to 90° minus
  the yaw it had and level it (the reset's `AngleWarpXNow` given the
  read-back yaw); the crystal's bag entry is put in world 123 at every
  tick in Atlantis (gone from the bar); the countdowns, the blinks and
  the shake count warp frames, which the original does not pace (the
  port: 60 a second). The port's only difference: a tentacle's file
  missing from every disc gets no slot (the original writes its frame and
  slot pointer one slot before the table; the game's data have every
  file). `tests/octopus.rs` checks the original's golden row by row
  (`tests/golden/octopus.txt`: the module's strings, floats, doubles,
  roll table and entities against the port's constants, `random`, the
  sectors, the entry per view, the shake, the tick on 76 states, the
  cursor, the dispatcher run on the golden's view names, the quit and the
  port's puzzle reset with the functions they enter, four fights pass by
  pass, the files and the crosshair drawn). Not compared: view 19's entry
  (no panorama on any disc), the puzzle tick with the pause byte (not in
  the port), and in the reset's rows the other modules' functions, which
  the golden's harness stubs (the test checks that the port's reset
  enters them in the original's order, the grids' leaves and the turtle
  ring's, the canoe's, the web's and the souls' quits, and that the
  web's, the ring's and the souls' clear their modes 1, 2 and 3; what
  the load's reset ports of the canoe's quit is told in the Saves item
  below).
  `tests/flow.rs` plays the fight with the mouse, lost then
  won.
  `tests/endgame.rs` runs every chapter's `/INIT` (and the trap), enters
  every view of the five places with their hooks, plays a firefly's
  move and its end, the fireflies won and the automatic way on, the
  vortex both ways (the second after a win on view 1's firefly), Rhea's subjects and the refused and opened link, the ring's
  rolling move (the direction kept, the roll undone, 76° after), a
  character's dialog, the star taken and laid (refused without it), the
  octopus fight's two ends, the hub from each world twice (the first trip
  counted once) and back, the hub's moves with their sounds and the
  crystals, the end (films, credits, film 99, the quit; a click in the
  credits), every dialog the chapters start (391-407, and Tibet's 372),
  the subjects, objects, musics and films on their discs, and saves in
  the four places and the hub; `tests/script.rs` compares every `/set`
  line of chapters 78-201 with the original's
  (`tests/golden/scripts_worlds_endgame.txt`, 209 rows, the trap's key
  written `{trap}`, compared up to its fatal error: the harness goes on
  where the game quits); `tests/flow.rs` `the_end_of_the_game_with_the_mouse`
  plays it with the mouse from Tibet (the three worlds' ends written as
  their scripts leave them; the fireflies lit in the mandalas' order),
  and `the_whole_game_with_the_mouse` plays **the whole game** with the
  mouse only, from the start-up intro and a new game to the end credits
  and the quit (Tibet, Ireland with its ball on the poet's island, Tibet's
  ball, China with its ball on the way back, Yucatán with its ball, the
  fourth first trip making the crystals, Xibalba, the six mandalas laid,
  Shamballa, Rhea, the ring, Atlantis, the hub's last trip; about 4
  minutes in release, 160 027 passes of 16.68 ms): every chapter the game
  goes through is left by one of its own `chapter=` lines (Atlantis's 82
  by the octopus fight's two ends), every puzzle function a player
  reaches runs, no hook is noted as not ported and no world refused, and
  every `/set` line reported is one the original runs; `tests/hd.rs` checks the hub, the fireflies'
  view, Rhea, the ring and Atlantis with the HD mode as the other places,
  and the HD view following the ring's roll. Not checked against the
  original running (Wine / Win98): the pictures, the roll on screen.
- **The canoe** (`canoe.rs`, `atl3d/ortho.rs`, `engine/canoe.rs`; the files
  in `atlantis2-data`'s `canoe.rs`; `bat.wam` view 402, chapter 35):
  loop type 9, the only puzzle with a loop of its own. The click branch
  of view 402 (`enable_pz_pirog` = 1) starts it (FUN_0049c010(1) →
  `pz_pirogue_init` FUN_00496a60): `(flag103=1)`, then the loading (the
  six hero databases `C1`, `g1`, `h1`, `a1`, `b1`, `e1`, the camera
  made with the first, the 33 depth tiles, the squares, the waypoints of
  `traj_pirog.txt`, the orthographic camera placed and turned, the depth
  factors, the seven stones, the river, `fdpuzzle.tga`, the shuffle, the
  board's depth), the pointer at the 640 x 480 surface's centre and the
  cursor pair `interfac.spr` frame 0x28; the loop type left is kept. Each
  pass is one frame of FUN_00498ca0 with the pointer, the left and right
  click flags (cleared by their handlers) and Escape (the key code 1):
  the square under the pointer, the clicks (take, lay back, swap, turn),
  the picture (the background, the board's depth into the 3D Z buffer,
  the tiles with the pointed one at 50 %, the stone counter, the stones
  and the hero in 3D, the held tile at the pointer with its stones, the
  pointer), Escape, the canoe's step (1.5 units a frame along the
  river; at each waypoint the hero's animation by its letter, a stone
  taken with `(NewSound=…)`, a break or the end without the seven stones
  in order stopping it), and the win (`finpirogue` = 1, and the first
  time `Pirogue_JustFinished` = 1 with the scan: chapter 35 sends the
  game to `xibalba.wam` view 124, chapter 39, a later win to view 404);
  then the quit (the cursor pair cleared) and the loop type kept. The
  river (`recalc_trajet` FUN_004988a0) chains the tiles' segments from
  `X9E1` by the nearest free end, a join of 20 units or more breaking
  it. Its errors (a segment starting on neither end, no direction, no
  free end left, the first waypoint moved) and a stone's waypoint not
  found are the original's fatal error (FUN_00480cd0: an emergency save,
  then the game quits): here the rebuild stops, the puzzle quits and the
  scripts stop with the original's message, which runs the same
  reporter (Scripts, below); the game's data never reach them. Two deviations there, on
  data the game never has: the original's copy of a segment has no bound
  (a segment without its end goes on past the table), the port stops
  with a fatal message of its own; the table the original logs twice
  before the "no free end" error (FUN_00498b40) is not logged. The loop
  has no clock: one step a frame (the original's frames are
  unpaced; here one a pass of the 60 Hz loop). The 3D is the original's
  Atl3D path for these databases (`atl3d/ortho.rs`): the node tree with its
  matrix round trips, the database slots and their track keys (the
  heads' `eval_track`), the **library of textures, materials and meshes
  by name** that every load adds to and that the canoe empties at its
  start (so every hero animation draws `C1.db`'s meshes, as the
  original), the orthographic camera of mode 2, the bounding sphere and
  clip codes, the back faces against the computed planes, the
  projection, the generic face drawer (which writes only x, y and w of
  the three vertex records: the texture coordinates are what the last
  polygon left) and the rasteriser of the CPU type (the heads'), the x87
  control word as the frame leaves it. `tests/canoe.rs` replays the
  original's emulated frames (`tests/golden/canoe_0e7f_p6.txt`, the
  Pentium II's routines under 0x0e7f with `spr_p6.dll`, `canoe_0e7f.txt`
  the same with `spr_p5.dll`, and `canoe_027f.txt`, the C ones under
  0x027f with `spr_p5.dll`): the loading's values (the init's command, cursor and control
  word, the camera's fields the port keeps; the waypoints by the MD5 of
  their table, the stones' and the slots' nodes by an MD5 each, so that
  the files hold no game data), then for each of 1797 frames the state,
  the script events, the picture's, the Z buffer's and the held tile's Z
  buffer's MD5 (and whole rows with pixel samples for chosen frames), a
  run quit by Escape, the win's frame count and variables and the cursor
  after the quit; all bit for bit; and the fatal errors on changed
  waypoints. At 800 x 600 the canoe draws on the
  640 x 480 surface, enlarged; with the HD mode on its frames are the
  original's and no HD layer is made (the front end enlarges the frame).
  The pointed tile at 50 % is drawn as `spr_p6.dll` draws it, the
  library a Pentium II loads (the reference run), or as `spr_p5.dll`
  does (its own run, both bit for bit). Not known: the
  held tile's depth written outside the 640 x 480 buffer (the original
  writes past its allocation near the screen's edges; the port drops
  those writes); the stale texture coordinates after other 3D drawing
  (a dialog's head) in the same run (the canoe's own drawing is checked
  from its start); the near and far clipping of faces (the canoe's
  objects never cross the planes: not ported, logged).
- **Two Atl3D paths.** The canoe's and the web's 3D were read and ported
  each for its own puzzle and checked on its own frames: `atl3d/ortho.rs`
  (databases, the orthographic camera, the generic face drawer) and
  `atl3d/warp.rs` (models over the panorama with the warp camera, the
  Pentium II routines, the Atl3D Z buffer), side by side under `atl3d`,
  both on the heads' helpers (`head.rs`: quaternions, tracks, node
  updates). What both go through in the original (the `.db` loader's
  face planes and bounding values, the node updates) is written twice;
  joining the two is a clean-up not done yet.
- **The spider's web** (`engine/web.rs`, `toile.wam` views 198-265,
  chapters 35 and 38; the module `pz_araig`): 65 nodes, views 201-265,
  four rings of sixteen and the centre, their positions read from
  `toile.vue` and their graph the views' destinations. The entry of view
  198 (@ 0x0048f530) loads the stage's 3D models (`atl3d/warp.rs`) and sets
  the puzzle mode 1; in it every entry runs the web's (@ 0x0048f9a0): a
  node loads the round (the creatures placed), the prey's node takes it
  (into the bag, into the hand the frame after), 199 or 200 leave it
  (the stage's `finaraignN` with the prey). At each arrival the spiders
  take a step (@ 0x004905c0, the chase @ 0x004908d0: the nearest node to
  the player or to the centre, or the next node of their ring towards
  the player's sector, by rings and sectors) and walk it frame by frame
  (@ 0x00491670, the puzzle tick: 0.02 of the edge and 0.07 rad of turn
  a warp frame); a spider ending on the player's node, or walked into,
  attacks, and at its attack's end the round is lost (the warp's draw,
  @ 0x00491b30: film 46, the prey taken back, the player put on 199 or
  200). A move click is taken while a spider walks or turns
  (@ 0x0048fdd0); with the feather in hand it lays it (the big spider
  turned back once), from 247 to 248 with the sword in hand it is the
  blow (film 48, four steps held); a click on a spider (its quads around
  it, @ 0x00491e40, picked at the screen's centre @ 0x00458fe0, before
  the zone's own click @ 0x0049caf2) asks about it (dialogs 187-189) or,
  with the sword, strikes it. No randomness. The creatures are drawn over
  the cube, under the cursor and the marker, by `atl3d/warp.rs`: the scene's
  objects posed by their tracks at the model's frame, the pipeline with
  the warp camera (76°, near 1.5) and the Pentium II's rasteriser, the
  prey and objects through its `Opacity` twin (black texels not drawn).
  The face planes come from the scenes' per-face normals (version 5),
  32-bit maps are read as 24-bit ones. A save made in the web loads in
  view 198 with the stage's variables from the prey in the bag
  (@ 0x0048f450); the reset quits the web. A `vue=` line lends the
  interpreter to the view's entry, so that the web's commands run inside
  it as the original's re-entrant `RunOneCommand` runs them (the exit by
  chapter 38's `vue=199`). With the HD mode on, the creatures are drawn
  again over the HD view by the same pipeline with the camera scaled,
  and the HD picture stands for the 640x480 frame with them (its base),
  so that the front end shows them at the HD size (`tests/hd.rs` checks
  what it composes).
  **Checked** against the original's code run under emulation
  (`tests/golden/web.txt`, `tests/web.rs`): the data and models as
  loaded, the step for every node pair, the entries, exits and load,
  the clicks, 2677 frames of passes, four rounds won from view 198 and
  20 frames drawn, bit for bit, and in them every object drawn (its clip
  codes, eye, vertex codes, face bytes and the used vertices'
  projections). **Not read**: what gives the web's objects a second
  texture-coordinate channel (bit 1 of the flags, from @ 0x004d6210: unset
  for the first object drawn in 14 of the 20 frames, set for every other
  one); the port draws with the mesh's UV set alone, and the 20 frames
  are the original's. **Not ported**: the clipping of a face
  crossing the near or far plane (none of the web's does; such a face is
  skipped and logged), the C variant's own rasteriser (the C variant draws
  the creatures with the Pentium II's routines). Not checked against the
  original running the web (Wine / Win98): the pictures on screen, the
  pace (the original's frames are not paced).
- **Maya counting grids** (`engine/grids.rs`, `pyratez.wam` views 55, 64
  and 75; chapters 31, 32, 34): each grid is 16 cells in four groups of
  four (cells 4k..4k + 3: the tests read only i mod 4; where a group lies
  on the screen is not read), a 16-digit string (cell i is digit 15 − i;
  save entries 21, 23, 25, `0000000000000000` at start-up, kept in the
  save's bytes the engine reads in place, as the module's states of
  entries 20, 22, 24), each digit the marbles in the cell. In a group the
  first cell takes 1..5 marbles, each next cell only when the one before
  holds five, the last at most four (@ 0x004935c0, @ 0x00493590) [inferred
  from that rule: a group is a Maya digit, bars and dots]. The grid is the
  frame list of `SPW_BASE5A` / `B` / `C`'s slot (80 frames, 5 i + n − 1
  for n marbles in cell i); the marbles in hand (0..5, not saved) are the
  hand pair's frame n − 1 of `spr\sprboul.spr`, a third bank of the hand
  pair (`HandBank::Puzzle`) drawn at the centre like an object. The entry
  (@ 0x00492f60, 0x00493070, 0x00493100) keeps the slot's id, resets the
  digits when `resetmayaN` is 1 (and sets it back to 0; `resetmaya3`
  resets grid 2's, a slip no chapter reaches), draws the grid (@
  0x00492ff0: the bank loaded, frames 0..79 out of the list, one frame per
  non-empty cell in, the puzzle mode 7) and sets the module's state 1
  unless 2. A click on a zone of type 50 (@ 0x004931a0 / 0x00493490 /
  0x00493510 through the step @ 0x00493220, nothing with an object in
  hand): 0x11 one more marble (four wrap to none) and 0x12 five or none,
  both with `sound100.apc`; 0x13 and 0x14 one less (none from five); a
  cell with marbles in hand lays them when allowed and takes back what the
  cell held (a swap, `sound100.apc` replayed by its handle, which is −1,
  with a warning and no sound, before any 0x11 / 0x12 of the visit), else
  `sound102.apc`; a cell with an empty hand gives its marbles to the hand
  (no sound) when allowed. When the click changed the grid and the digits
  equal the solution (`0000000000000045`, `0000000000040001`,
  `0000155500350001`: 9; 4·20 + 1; 16·400 + 8·20 + 1): the state 2,
  `finmayaN` = 1 and `sound101.apc` on voice 2; the click scan of the same
  frame follows (chapter 34's crocodile opens there, its sound 112
  replacing 101). The mode 7's tick (@ 0x00493660, every warp frame) sets
  the hand pair from the marbles while `OBJETENMAIN` is 0. A move zone of
  the grid's view runs its leave (@ 0x00492d60..): the state 0 unless 2,
  `finmayaN` = 0 when the digits are not the solution, and in that view
  the hand pair emptied, the bank freed, no marble, the three sounds freed
  on voice 2, the mode 0; a load runs the three leaves first (the reset @
  0x0049bf70), so a load made in another view keeps the marbles in hand
  (not saved; only the drop and the step write them, @ 0x00492e0a,
  0x00493249..0x0049345c). Kept from the original: the mode 7 outlives a
  view left by a script's `vue=` (the marbles stay in hand), a leave with
  an object in hand empties its picture, the solved grid stays solved
  whatever is laid after. Against the original's emulated run
  (`tests/grids.rs`, `tests/golden/grids.txt`): the data; the entries per
  view, state and reset (twice: the bank already loaded); the step on
  every cell for each hand 0..5 and each digit of the cell, the one before
  and the one after (18576 clicks: the digits, the hand, the changed byte,
  the sounds, the slot's list); the other zones for each hand −1..6 and
  object, a sequence with the sound table kept, the two tests on six
  strings; the shortest wins of the three grids and a played way to the
  first (a refusal, swaps, a take back, the hand put back), clicks after
  the win, the leave, the entry again; the leaves in and out of their
  views; the cursor for each count, object and bank; the dispatchers and
  the reset; the clicks through the engine's pass with chapters 31, 32 and
  34 (the hooks, `CLICZONE`, the lines of the scan, the sounds, the state:
  the crocodile's section in the win's frame); the hand of marbles at the
  centre and its five sprites drawn; the three grids' slots drawn for
  seven digit strings and all 80 frames alone (md5 of each face, pixels).
  `the_grids_played_with_the_mouse` plays the three with the mouse (the
  centre steered onto each zone, a click), the first with a refusal, a
  swap and a take back, to their wins, the crocodile opening in the third
  win's frame (chapter 34's lines 25-33); a save keeps the digits, loaded
  in the grid's view (the hand dropped) or in another (the hand kept).
- **The figurines on the floor** (`engine/figurines.rs`, China's
  `miniature.wam`, `SS` views 1-7; chapters 55, 47 and 48): six
  figurines lie in the grass of view 13 (the objects `OB_FIG1HERBE` ..
  `OB_FIG6HERBE`, taken by a click as any object) and are laid on the six
  places of view 6's floor (the zones `SOL_FIGUR1..6`). On the floor,
  figurines 1-5 are the characters `FIGURINE1..5` and the sixth is the
  object `OB_FIG6HERBE`, each a slot with one frame per place. The state
  is the scripts' (`pos_statue1..6`, the place of each, 0 = not laid;
  `etat5statues`); the slot indexes the entry keeps (@ 0x006d18b8) are
  never saved. The entry FUN_0048d680 (every `SS` view 1-7, after the
  slots are loaded) shows each laid figurine at frame place − 1, its
  animation stopped (the slot's playing byte +0xbc cleared), and hides
  the others; in view 7 a figurine on its own place (targets
  @ 0x005e1084: 4, 3, 5, 1, 2, 6) takes the slot of its animation 2
  instead (FUN_0047dea0) and plays it from frame 0: the figurines come
  alive there, and chapter 48 makes them talk and trade the coins for
  the wheels' discs. The click FUN_0048da20 (view 6, any zone, never
  taking the click; nothing once `etat5statues` is 1 or on a zone whose
  name does not start with `SOL_FIGUR`, the place digit its tenth byte):
  a figurine on that place is taken (in hand and in the bag, the hand
  and cursor on its sprite, `(pos_statue<k>=0)`) unless it is on its own
  place, which ends the click; then a figurine held before the click is
  laid there (out of the bag, `(NewSound=202)`, `(pos_statue<k>=<d>)`),
  the hand emptied unless a figurine was taken with something held (a
  swap keeps the taken one in hand). All six on their places:
  `(etat5statues=1)` (which no chapter reads) and `AtlHnm_start("???")`,
  which opens nothing (the port's video start: no such file, the music
  back to 100 %). The slots gained the original's +0x34 (`Slot::single`,
  from the state's +0x2c at load) and playing byte (`Slot::playing`, set
  at load; a slot without it keeps its frame at the tick, @ 0x0047d82c).
  A `vue=` line runs the view entry while the interpreter is out of the
  engine: `Host::enter_view_with_vars` lends it the script variables for
  the entry's puzzle set-up. Checked against the original
  (`tests/figurines.rs`, `tests/golden/figurines.txt`: the slots and
  zones of views 1-7 and 13, the entry per view and state, the click
  along a played sequence with its swaps and refusals, the check, the
  dispatcher, the warp frame with chapter 47, the step in view 7 and the
  draws of the figurines' frames, the frame equal to the file's count
  included) and played with the mouse (the six taken in view 13, the
  walk to view 6, laid with a swap and the refusals, the walk to view 7
  with film 23, the figurines alive, the sixth's coin and the first
  figurine's dialogs 281 and 282 for its disc). Quirks kept: in view 7
  the sixth figurine's slot is always its animation 2 (a one-frame file
  loaded first), so off its place it shows frame place − 1 of a
  one-frame file, which draws nothing; the bag's byte of a taken
  figurine is its sprite; two figurines on one place (a crafted state)
  are both taken by one click.
- **Chinese chest** (`engine/chest.rs`, `miniature.wam` views 1-7,
  chapter 47): the chest `COFFRE_CHINE` has one slot in each view, one
  animated state of five frames (4 shut, 0 open), which the puzzle holds
  on frame 4 − `frame_coffre_chine`. The view entry (@ 0x004938f0, from
  the entry dispatcher for `SS` 1-7) finds the chest's slot (the first of
  its id, any kind, @ 0x00493aa0; none is the original's fatal
  `pz_coffre vue %d ! find_idx_pzcoffre(%d) introuvable`), sets the
  puzzle mode to 5 and holds the slot: its playing byte cleared, its
  frame set. The click (@ 0x00493960, from the click dispatcher on every
  zone click of `SS` 1-7, before the zone's type is read, never taking
  the click): in view 5 the chopstick's slot is found too, and on the
  zone `PRENDRE_BAGUETTE` (by its value) the counter goes up by one with
  `RunOneCommand("(NewSound=%d)", 210 + counter)` while below 4, the
  chopstick `BAGUETTE` shown at 4 (FUN_0048c610: its slot and the show /
  hide table); from 4, while `BAGUETTE_PRISE` is 0, the chopstick goes in
  the bag and in hand (`OBJETENMAIN` and the hand pair; the cursor pair
  untouched), its table row hidden (the table only), its slot made
  inactive, `BAGUETTE_PRISE` = 1; then in every view the chest held
  again. The slots now have the original's playing byte (+0xbc, 1 at
  load): the 15 Hz step moves only the active slots that play. **The
  puzzle mode** (@ 0x006d84d0, save entry 26) is engine state: 5 has no
  tick, and the click dispatcher, on the first zone click in an `SS` view
  outside 1-7 with the mode 5, clears it and runs no hook on that click
  (the wheels' first click in view 13 is lost, as in the original);
  a new game keeps a 5, as the original's (it does not run the puzzle
  reset). Three other entries overwrite the mode, as in the original:
  the turtle ring's for views 40-64 (2), the clouds' in `SS` 31 (4,
  unless `FINPERLE` is 1) and the souls' in `LA` (3); so a save made in
  the turtle room after the chest writes entry 26 = 2, and the first
  click in `SS` 31 is eaten only once the pearl is won (the clouds'
  entry then leaves a 5 in place). Against the original's emulated run
  (`tests/chest.rs`, `tests/golden/chest.txt`): the entry by view,
  counter and mode, with no slot; the click alone along the five clicks
  and their variants (types, views, the chopstick already in the bag,
  another object in hand, counters 3, 5, 7, −1, −3, `MONDE` 5, no
  chopstick slot, views entered after the show and the take); both
  dispatchers with the mode; the click handler and chapter 47's click
  scan in view 5 and then with the chopstick on the four incense sticks
  to film 23 and chapter 48; the save entry; the chest's seven files and
  the chopstick's drawn (faces' md5s and pixels); the step on the held
  chest. A game is played with the mouse from chapter 48's view 7 into
  chapter 47, the chest opened and the chopstick taken in view 5, the
  sticks lit with it back in view 6 (chapter 47's lines 44-47, 56-59,
  68-71, 80-89) to chapter 48.
- **Pearl and dragon** (`engine/dragon.rs`, `miniature.wam` view 31,
  chapter 58): a dragon (`DRAGON_NUAGE`, one animated state of 80
  frames, held on frame 50), two clouds with a pearl behind each, five
  low clouds (frame 1 raised, 0 down) and the pearl with a hole
  (`PERLE_TROU`, an object to take). The view entry (@ 0x004944a0)
  keeps the slots of the low clouds and of the dragon (the first slot of
  each id; none is the original's fatal `pz_dragon find_idx(%d)
  introuvable`), hides both pearls, holds the dragon (frame 50, its
  playing byte cleared); with `FINPERLE` = 1 it raises the five low
  clouds and stops there; otherwise it sets the puzzle mode 4, restores
  a transparency in progress (`Time_nuage_transparent`,
  `NuageTransparent`), places the low clouds from `State_nuages_bas` and
  hides `PERLE_TROU` (its slot and the show / hide table). The click
  (@ 0x00494600, zone types 3, 4, 0x32, never taking the click) needs
  the mode 4, no transparency, the dragon still and `FINPERLE` = 0; the
  zone is told by its name, byte for byte: before the first round
  `CLIC_DRAGON` starts it; then `CLIC_NUAGE_GAUCHE` / `_DROITE` set
  `Action_nuage_bas` (1 when the pearl hides on that side, `Perle_cachee`
  'A' left, 'B' right, else 2) and make that cloud transparent for 30
  frames (+0xc0 = 0x32: the slot is drawn only while the 15 Hz counter
  is odd, @ 0x0047d181..0x0047d1aa), its pearl shown when it is there. A round
  (@ 0x00494a40(1)) plays the dragon once round and hides the pearl
  after the fixed sequence `ABBAB` (`Num_sequence_dragon` 0..4, wrapping).
  Not ported: a `Num_sequence_dragon` below 0 or above 7 (only a save
  written by hand has one) gives `Perle_cachee` 0 with a warning, where
  the original reads the byte that far from its table with no bound
  (@ 0x00494a73: the module's next strings above, whatever precedes the
  table below; 5..7 read the string's zero bytes in both).
  **The puzzle mode and its step**: the mode (@ 0x006d84d0, save entry
  26) is engine state; the clouds' move zones set it to 0
  (@ 0x004945f0), and the click dispatcher clears a 5 (the chest's
  mode, from a save here) on a zone click in an `SS` view outside 1-7,
  running no hook on that click (@ 0x0049b96d..0x0049b984); nothing
  else here (a new game keeps it, not running the original's reset);
  the other modes' steps (1, 2, 3, 6, 7: a save made in their rooms)
  are noted each frame (`Event::PuzzleNotPorted`); while it is not 0,
  every warp frame runs its step after the move's completion, turning or
  not (`Engine::puzzle_tick`, FUN_0049bfb0 @ 0x0045a5e8; the original
  skips it while paused, which the port has not). Mode 4's step
  (@ 0x004947b0) moves the dragon's state 1 → 2 → 3 one frame each and
  3 → 0 at a frame that finds it on frame 50, then counts the
  transparency down; at its end the pearls are hidden, the cloud drawn
  again, the low clouds raised for a right answer or lowered for a wrong
  one (above 0) with `(NewSound=%03d)` of 205 + the new count, and at
  five `FINPERLE` = 1 and `PERLE_TROU` shown; while fewer than five are
  raised the next round starts. **Timing**: the step and the 30-frame
  transparency count warp frames, which only the screen's refresh paces
  in the original (one a refresh in full screen; the port: 59.94 a
  second, half a second of transparency); the dragon's frames move with
  the 15 Hz step of the place animations. So a dragon woken by a click
  whose next two warp frames see no 15 Hz step is stopped on frame 50 at
  once, without its flight (at the port's rate, a tick every 4th pass,
  one click in two; the rounds that start by themselves have three
  frames), as the original at any frame rate above 30
  [CONFIRMED-EMULATED]; the flight gives no hint of the side anyway (one
  animation for both). Unknown: whether the machines of its time drew
  view 31 within a refresh, so how often its players saw the flight
  skipped (to settle: time `AtlWarp_continue` in the Win98 VM). The slots
  now have the original's playing byte (+0xbc, 1 at load: the 15 Hz step
  moves only the active slots that play) and the blink word (+0xc0, 0 at
  load). Not ported: an index kept from view 31 that names no slot of
  the current view (the mode 4 outliving the view: a new game from the
  menu during a flight) reads and writes nothing, where the original
  uses the record left in its table. Against the original's emulated run
  (`tests/dragon.rs`, `tests/golden/dragon.txt`): the entry by
  variables and mode, without each kind of slot; the click alone along
  its sequence and variants (names, types, values, modes, the sequence's
  wrap); the step alone through rounds, wins, wrong answers, the edges
  of the counts and the step in view 32; frames run at five rates of the
  15 Hz tick (1, 2, 3, 4 and 8 frames a tick) with a player answering,
  row by row; both dispatchers and the per-frame one; the clicks with
  chapter 58's scan to the pearl laid in the mechanism (film 26, chapter
  59); view 31's twelve files drawn with the blink; the dragon's flight
  step by step. A game is played with the mouse from chapter 56's view
  18 to view 30 (chapter 58) and 31, the dragon woken, seven rounds (one
  wrong answer), the pearl taken, then laid in view 32's mechanism from
  the bag (chapter 58's lines 19-27, film 26) to view 19 and chapter 59;
  a save keeps the transparency, the counts and the mode.
- **Turtle ring** (`engine/turtle.rs`, China's turtle room,
  `salletortue.wam` views 40-63, chapter 46): an engraved ring of 39
  frames (`ANNEAU_GRAVE`, a place animation of every view of the room)
  that the zones `TORTUE_GAUCHE` and `TORTUE_DROITE` of views 56-63 turn
  a quarter, one frame per 15 Hz tick, with sound 218. The state is the
  puzzle mode (2, @ 0x006d84d0, save entry 26), the pending turn count
  (@ 0x006d84e0, entry 28) and the ring's frame (@ 0x006d84e4, entry 29),
  kept in the save's bytes. The view entry of `CH` views 40-64
  (@ 0x0049c340) sets the mode 2 (and, when it was not 2, the frame 0),
  then holds the last slot of the ring's id (its playing byte cleared, so
  the 15 Hz SPW step skips it: `anim.rs` `Slot::playing`, +0xbc) at the
  kept frame when below its end; outside 40-64 it gives the mode 2 back.
  A click on a zone of type 3, 4 or 50 (@ 0x0049c290, from the click
  dispatcher, which never takes it) does nothing while a turn is pending;
  else `TORTUE_GAUCHE` sets the count 9 (10 when the frame is not 0) and
  `TORTUE_DROITE` −10 (−9 from frame 9), each with `(NewSound=218)`; the
  mode is not read. The warp frame runs the puzzle tick (@ 0x0049bfb0,
  `Engine::puzzle_tick`) after its moves and scans whenever the mode is
  not 0; the ring's (@ 0x0049c400) takes the frame dispatcher's 15 Hz flag
  (@ 0x00656a50, `Engine::frame_flag`: set by each tick, cleared after
  every pass) and moves the ring one frame towards a count of 0 (0 after
  38, 38 before 0), the frame kept; outside 40-64 it gives the mode back,
  and with no ring in the view it is the original's fatal error. So the
  left zone rests the ring at 0, 9, 19, 29 in turn and the right one back,
  and the ring keeps its frame from view to view and in a save. No script
  reads the ring: it is the player's aid for the turtle master's
  indications, and the room's win is chapter 46's count of arrivals (six
  indications, one an arrival in the room, the seventh arrival silent,
  the eighth in view 47: the film and `miniature.wam`). Three quirks
  stay: a click during a turn is lost; the mode, count and frame survive a
  new game (the original's puzzle reset runs at start-up and at a load
  only); view 58's water zone is of type 10, which the WAM load does not
  name, so its value stays 1 and the turtle cannot be laid from there.
  `tests/turtle.rs` checks the entry, the click, the tick, the
  dispatchers' choice, the clicks with chapter 46's scans, the arrivals of
  the indications to the win and the ring's 25 files drawn (every frame
  of views 56-63, five of the others) against the original
  (`tests/golden/turtle.txt`), plays the room with the mouse from
  `chine.wam` view 32 in chapter 43 (the meeting, the ring turned both
  ways, the turtle laid, the master asked about the tiger's home, his
  indications walked to view 47, chapter 47; the room's moves make a
  two-coloured graph, so the master is asked from view 57, an even walk
  from 47), and loads the ring from a save.
- **The labyrinth's souls** (`engine/labyrinth.rs`, `labysol.wam` views
  1-30 and `labyplaf.wam` 31-60, chapter 60): four wandering souls
  (`AME1..4`, one per corridor direction, their entity's SPW prefix
  `1a`..`1d`) whose states sit in two template views (509, 510) at a
  distance (animations 1-3). The view entry's hook (@ 0x0048df10, after
  the view's own slots) sets the puzzle mode 3, finds the view's cell in
  one of two 13 x 11 maps (@ 0x005e1184, 0x005e1214; the port keeps
  which bytes are spaces), tests every state of the place with the
  original's 20 corridor tests (open and walled cells around the cell, a
  direction, an animation, a template view; in views 1-30 the
  directions a ↔ c and b ↔ d, the floor's map turned round), drops one
  candidate at random for 3-4, two for 5-7, three from 8 (`random(n)` =
  `rand() · n / 32767`, a budget of 100 misses), loads the others after
  the view's slots, held and hidden, each with a delay (`random(max(0,
  (150 − end) · 5) + 45) + 10`, drawn again from `… + 105` while within
  44 of another soul's, ten tries). The puzzle tick (@ 0x0048eb30, every
  warp frame while the mode is 3; outside views 1-60 the mode back to 0)
  counts each soul's delay down **per frame**, then plays its passage
  once with `(NewSound=220)` and hides it at its last frame with a new
  delay and `(NewSound=221)`, a sound on no disc. A passage of
  animation 3 has its click kind cleared in the frame after it appears,
  when its frame (0 or 1) is still empty: `labyplaf.wam`'s souls, a
  character in the data, are never clickable, and chapter 60's talks
  with them never run, as in the original. The delay is kept in the
  slot's start frame, as the original does; slots now carry the playing
  byte (+0xbc) and the WAM record they were loaded from (FUN_0048eae0's
  look-up). The draws are the engine's `rand`, the one the dialogs and
  the music use (seeded by each music start). With it the puzzle mode
  (save entry 26, read in place) and the puzzle tick dispatcher
  FUN_0049bfb0. Checked against the original (`tests/labyrinth.rs`,
  `tests/golden/labyrinth.txt`): every view's cell and neighbourhood,
  every view's candidates in both places, the tests on 403 synthetic
  neighbourhoods, `random(n)` at its edges, the entry in all 60 views
  for three seeds (slots, delays, the generator's state after), its edge
  cases, the delay alone (the ten tries), the tick alone, the warp frame
  pass by pass, a click on a soul, the dispatchers, and the souls' 20
  files drawn; then chapter 60 played with the mouse (the door from
  `miniature.wam`, a soul's passage, the buffalo's empty form, the fox
  woman's fan read, the three demons' stamps in the fan's order across
  both places, the buffalo's answer: dialog 322, the mushroom,
  `flag173`). Known differences: the place animations' hand-over draws
  from their own generator here (the fox woman's weighted states in
  views 20, 21 and 30 share the souls' generator in the original); a
  drop draw of `rand()` = 32767 reads, in the original, an uninitialised
  stack word past the candidates (a 1 there drops nothing more): the
  port takes it as a miss (not known what the game's stack holds there;
  to settle, a breakpoint at atlantis2-cd-fr-2199552 @ 0x0048dff2 in a
  Wine run of the labyrinth, the word read when EAX is the candidate
  count); the original's fatal errors, which the data never reach,
  stop the scripts here (34 candidates) or are logged (AtlSpw_load's
  full slot table, as for every place animation).
- **Ogham stones** (`engine/ogham.rs`, Ireland's view 6 of
  `bigile1.wam`): fifteen places in three rows of five on a standing
  stone, the frame list of `OGHAM_SPRITE`'s slot (frame 4 k + e: place k,
  engraving e; 0xff no stone). The state is 15 bytes (@ 0x005e20dc, save
  entry 31), every place with engraving 2 at start; nothing resets them
  but a load (a second new game in the same run keeps them, as the
  original's does). The view entry (@ 0x00496020) keeps the sprite's
  object index, takes frames 0..14 out of its list where there, and adds
  the 15 bytes in order, so that list byte k is place k. A click on a
  zone of type 3, 4 or 50 with `COUTEAU_AILILL` in hand (@ 0x004960a0,
  from the click dispatcher, which never takes an Irish click) acts by
  the zone's value: 99 compares the bytes with the target (@ 0x005e20ec:
  two stones with engraving 0 in row 0, five with engraving 3 in row 1,
  none in row 2) and, when equal and `FINOGHAM` is 0, sets `FINOGHAM` = 1
  with no scan (chapter 15's click scan of the same frame then plays film
  3, shows the ogham stick and goes to view 106); 16, 17, 18 turn a row's
  engraving (the first place's frame + 1, through @ 0x0047ca50, wrapping
  within the four; the row's stones take it, + 4 a place); 1, 6, 11 bring
  back a row's first stone with engraving 2 when it and the next are
  gone, or take it when the next is gone; 5, 10, 15 take a row's last
  stone or bring it back as the previous place's frame + 4, whatever that
  is; the others (2-4, 7-9, 12-14) bring a stone back next to a stone on
  its left or take it when the place on its right is empty. Every value
  from 1 runs `RunOneCommand("(newsound=4)")`; the list and the bytes
  change together. Its move-zone hook (@ 0x00496080) clears the cursor
  pair. Against the original's emulated run (`tests/ogham.rs`,
  `tests/golden/ogham.txt`): the entry on fresh, stale, two and no slots;
  each zone value from the start and with no slot; every reachable row
  state with its six moves (66, 71 and 71 states, 332706 in all), found
  again by the port's own breadth-first search with the golden's checks
  and independence sample, and the shortest solution (11 clicks, the
  same path); the win and the checks after it; the dispatcher's choice
  by zone type, hand and view name; the drawing of nine lists into six
  faces (md5 of each face, pixels). `the_ogham_stones_played_with_the_mouse`
  plays it from chapter 3's view 7 with the knife in the bag: the walk to
  view 6 (chapter 15), the knife picked from the bag bar, zones 99 and 17
  on the start's bytes (golden G: no win, nothing run), each of the 11
  clicks checked against the original's move, a save and a load, zone 99
  with the stick already in the bag (golden G: `FINOGHAM` = 1, nothing
  run), the save loaded again, zone 99: `FINOGHAM` = 1, chapter 15's lines 16-19 (the original's, golden
  section G), film 3, view 106 with the stick shown. Kept from the
  original: the last place brought back from an empty previous one gets
  frame 3 (row 0's first place), a row turned while its first place is
  empty takes row 0's frames, sound 4 on every click, the clicks use the
  index the last entry kept. Not ported, as no data reaches them (view
  6's zones have the values 1-18 and 99): the generic branch on a value
  of 19 or more other than 99 (the original writes byte 0x005e20db +
  value with no bound, into the target @ 0x005e20ec and what follows,
  and from value 128 reads list bytes past the list's 128, the slot's
  next fields and the next slot's record: the port only logs the write
  and reads 0 past the list), and a click with no `OGHAM_SPRITE` slot
  (the original edits the record past the last slot, and the edit stays
  for the next click: the port edits a zeroed list it then drops).
- **Blackbird songs** (`blackbird.rs`, `engine/blackbird.rs`): the poet
  `POETE_ASSIS` of `ilepoete.wam` view 73 (chapter 20) asks which of five
  blackbirds sings the song he names. The view's entry (@ 0x004958f0)
  loads the two answer sounds `merlem.apc` (wrong) and `merleb.apc`
  (right) into the sound table the place sounds use (50 entries, a
  handle being an entry's index), resets the counter and the five song
  handles, draws the five songs (1 + `random(10)` each, drawn again while
  equal to an earlier one, @ 0x004959a0; `random(n)` = `rand()` · n /
  32767, so n itself when `rand()` = 32767, @ 0x004958c0), sets the
  cursor pair `interfac.spr` frame 40 and the pointer at the centre. The
  game's click (@ 0x00495ae0) is the first thing of every click in that
  view, whatever is open (@ 0x0049ca8e). A click on the poet opens his
  subject menu (three subjects) and, as he is `CLICPERSO` with
  `FINMERLE` = 0, sets mode 1; a subject clicked with the pointer on the
  game's own three-row grid (one column of 62-pixel cells from (419,
  149): 15 pixels off the menu's cells, both far edges included) starts
  a round: the right bird drawn (`random(5)`), the five songs
  `merle<n>.apc` loaded (each old handle freed first, which stops voice
  2), the poet's question `(dial=89 + song)`, mode 2, the pointer at the
  centre. In mode 2 the warp draw shows the five birds (frames 26..30 of
  `sujeire.spr` centred on (475, 205 + 62 k), their rings) instead of the
  subjects, and the bird under the pointer sings on voice 2 whenever
  voice 2 is free (@ 0x00495a70). Any click then answers (the subject
  menu's answer turns mode 2 into 3 and leaves the click to the game,
  @ 0x0045ab69..0x0045ac72): the bird under the pointer against the
  right one, `merleb.apc` and the counter + 1, or `merlem.apc` and the
  counter back to 0 (a click outside the birds is wrong too); three right
  answers in a row set `FINMERLE` = 1, which chapter 20's click scan of
  the same frame turns into film 14, the standing poet and dialog 73.
  Leaving by a move zone (@ 0x004959f0) frees the sounds (stopping voice
  2), sets the music to 100 % and clears the flag (@ 0x006d2751) that
  keeps the cursor choice off during a round. The mode is neither saved
  nor reset by a new game or a load. The quirks are kept: the subject
  menu's edge column chooses a subject without starting a round, and the
  mode stays 1 with the menu closed, so the next click with the pointer
  on the grid starts a round; `random(5)` = 5 makes the question read
  the right sound's handle as a song (dialog 89 + handle, or a fatal
  error past 10); a song 11 has no file (a fatal error). At these fatal
  errors the original makes its emergency save and quits (FUN_00480cd0);
  the port does not: it logs the error and stops the scripts, and the
  round goes on with the other songs. The draws share the engine's `rand` (seeded by the music's start and
  the dialogs, as in the original; the original's place animations draw
  from it too, here they have their own). Against the original's
  emulated run (`tests/blackbird.rs`, `tests/golden/blackbird.txt`):
  `random` for every `rand()` output, the songs from 22 seeds, the entry
  on five sound-table states, the click in every mode (the poet test,
  the round's grid and right bird for each `rand()` boundary, every song
  value, the answers for each right bird, pointer and count, the
  `random(5)` = 5 case on three table states), the song under the
  pointer, the bird menu's draws, the leave, and 58 warp frames of a
  game with chapter 20 (the menus, the songs, the answers, a wrong one,
  the edge column, the win with its film and dialog) pass by pass, with
  their sprite draws, scans, sounds, the click flag and the cursor; and
  a game played with the mouse by ear from chapter 20's arrival on the
  island to the standing poet's first subject (dialog 64). The handles
  are those of the one sound table the place sounds, `NEWSOUND`, the
  films' and moves' tracks and the puzzles share (Sound, below), so a
  `NEWSOUND` entry the original leaves loaded shifts them as it does.
- **Worlds:** Tibet (`tibet.wam`, world 5), Ireland, China, Yucatán,
  Xibalba, the space-travel hub, Shamballa and Atlantis (above) are
  ported. A `changewam` of the scripts to any other place (Ireland's
  `demo.wam`, chapter 1's, whose panoramas are on no disc and which no
  chapter reaches, or `sans_nom.wam`, the one place file no script and
  no link names) is
  refused: where the original loads it (a place it cannot load is fatal
  there), the port notes it (`Event::WorldNotPorted`), ends the section
  at that line, sets the world back to the current place's (`Interp_SetMonde`: the
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
  (`[ObjectNameList]`, the keys upper-cased, a repeated key keeping its
  last name, a missing one an empty text; the original's tokenizer: every
  line of the block is an entry, the name the rest of the line after the
  blanks, its trailing blanks kept; `atlantis2_data::messages`, against
  the original's loader run on the installed file and on a synthetic one,
  `tests/input_ui.rs`) is drawn in
  `AtlFT_POPUP` (10 points) centred on the cell, kept on the screen, its
  top at H − 70, black one pixel down and right then white; a click in
  the bottom 80 rows on cell x / 80 puts its object in the hand and the
  cursor, sets `OBJETENMAIN`, stops the turn, uses the click and closes
  the bar. While the bar is open the centre draws nothing, the cursor is
  not chosen, no sprite under the centre is recorded (FUN_0045b040 @
  0x0045b0cd), so a click takes no object, sets neither `CLICOBJET` nor
  `CLICPERSO`, moves nowhere and runs no click scan; and the warp frame
  runs neither the mouse look nor a move's turn step nor the zone pick
  under the centre (`AtlWarp_continue` @ 0x0045a3b0..0x0045a3ea, also
  while the subject menu, a dialog or a full-screen overlay is open): a
  move's turn waits under the open bar and goes on when it closes
  (`tests/ui_fixes.rs`, against the original run under emulation). The
  zone under the centre is still picked on every mouse move: the motion
  callback (@ 0x0040c420, its end @ 0x0040c4df) picks it in every loop
  type whatever is open, so after a view entry under the bar it follows
  the camera at the next move.
  `cinema=` closes the bar (its flag only, @ 0x00481d4e). The object in hand is drawn at the screen centre under the
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
  game's files in the same test file. Not known: the text's two
  colours (the calls pass 0 and −1, taken as black and white as for the
  subtitles); the frame of a stamp or symbol whose value is outside the
  executable's table (the original reads past it; the port draws
  nothing). The bar scrolls one entry when more than the float 0.1
  (0.10000000149 s) has passed since its last scroll.
  The first frame after the bar closes shows its pointer frame at the
  screen centre, as the original's cursor pair still holds it until the
  next cursor choice. The main, CD and player menus set the cursor pair
  to their own pointer (`curseur.spr`, frame 0) and leave it so when they
  close: with the bar open across a menu, the bar's pointer is the menus'
  until the bar closes, and a save made from the menus records the cursor
  frame 0 (`tests/input_ui.rs`).
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
  (`sound.rs`): each voice converted to the primary buffer's format,
  22050 Hz stereo (FUN_0040f480), and summed there; that mix is then
  converted to the device's rate. Both conversions interpolate linearly,
  so the four 44100 Hz files keep one sample in two, unfiltered (what
  they hold above 11025 Hz folds back below it). How the players'
  DirectSound converts (a Windows 9x software mixer, a card's mixer, a
  later Windows' mixer) is not known, so this part is the port's choice.
  Without a sound device the window still plays the mix, into nothing,
  at the game's pace, so every voice ends at its length as DirectSound's
  buffers do (a `NEWSOUND`'s entry is then freed as with a device; the
  original without a sound card plays nothing at all). Every sound but the music and the
  dialog voices goes through one table of 50 entries (FUN_0047a530: a
  name already loaded is reused, else the first free entry; a full table
  is the original's fatal error) shared by the place sounds, the puzzles,
  `NEWSOUND` and the tracks, with one handle (@ 0x00673f34) for the
  last `NEWSOUND`, film's or move's track: a `NEWSOUND` while voice 2
  plays frees the handle's entry first, a track loaded over a
  `NEWSOUND` that still plays leaves the `NEWSOUND`'s entry loaded for
  good, and each pass frees the handle's entry once a `NEWSOUND` has
  been played out (FUN_0047a2f0), as `tests/sound_glue.rs` checks against
  the table of the original's runs. The place sounds (`place_sound.rs`, `engine/places.rs`)
  are the original's: the sources of the place file (a position, a
  level, four files with a probability each) heard on voices 3 to 10
  from the listener position that a table in the executable gives each
  view of ten places (read from `atlantis2.exe`; a place not in it, as
  Tibet, has none). At each view entry the sources the view hears take
  one of 8 slots unless one holds them already (a slot keeps the view's
  q it was taken with), their files loaded once by name; each panorama
  frame that is not turning sets each playing slot's volume and pan from
  the two ears' gains (the float arithmetic transcribed under the x87
  control word) and lets each silent slot draw a file (`rand()`, the
  probabilities) or wait a random number of 15 Hz ticks; a sound starts
  silent and gets its level the next frame, and a source whose first
  probability is 100 loops. Along a video the listener moves from the old
  view's position to the new one's, one step per frame, and the sources
  the new view does not hear fade over the last two thirds; its end frees
  their slots, while a move without a video keeps every slot (heard from
  the new listener). A place file loaded frees them all. The gains, the
  levels and pans under the three control words, the video's steps and
  fade, a sound's start and the scheduler are the original's
  (`tests/place_sounds.rs`, its emulated runs), and so is every bruitage
  of the shipped places at eight yaws. Ireland's `bigile1.wam` is the
  only Irish place with sources (8 sources, 33 views); Tibet's two
  transition videos have no sound track. A film's track (`cineNNN.apc`) plays on voice 2 at the
  effects volume from the film's first frame and stops with it; after a
  film the next panorama frame asks for the music at 100 %, unless a new
  music starts first (a `startmusik` right after the film: the old music
  fades out without swelling back first). Voice 2 is the video's only
  when its track loaded, or for a move's `TrajSound` (even one that does
  not load): only then does the video's end stop voice 2, so a
  `NEWSOUND` still playing goes on through a film without a track (18,
  33, 60, 67) or a move without a `TrajSound`. A video that does not
  open keeps the loaded track for the next one (film 4: its track is on
  disc 1, its video on no disc). Compared with the
  original's own functions under emulation
  (`tests/golden/sound_glue.txt`, `tests/sound_glue.rs`). The music state
  machine also runs inside the original's long loads (FUN_00476e10, again
  while it writes the ring): twice in a video's start, at a video's end,
  before each transition sprite bank, eight times in a panorama's load
  and before each place animation of a view; so a music asked for before
  a view's entry or a video opens there, in that pass. The fades' float
  arithmetic runs under the process's x87 control word of the moment
  (0x0e7f once the Pentium II warp has drawn), not the start-up one.
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
  second). The files are read from the discs: the original plays
  hard-disk copies (the install's `music\` files are empty) that
  `LOADMUSIK`, or a music started whose copy is empty, makes from the
  disc in the drive under the "please wait" page; the port copies
  nothing but keeps which copies the original would hold and shows the
  page where it copies (Menus, below). Every track decoded from its start and from each of the
  83 marks, the random marks and the list choices are the original's
  (tests/music.rs), and so is each start's voice: created from the music
  buffer, looping, its cursor and volume (`tests/music_player.rs` against
  the runs' `init` rows). A random mark is drawn as `rand() · (marks + 1)
  / 0x7fff`, so `rand()` = 0x7fff gives one past the marks, which
  `AtlMuzik_GotoMark` reports as a fatal error (the original saves and
  quits; Native stops the music and the scripts); with the minor bug
  fixes the track plays from its last mark instead. A load puts the
  save's `startmusik` list back (it is the player's own list) even when
  the save's music is 0 and the music playing goes on. The music player
  draws from the game's one `rand` (Place animations, above).
- **Alpha masks:** the dialog videos carry an `AL` chunk per frame (runs of
  (n, a) pairs, a = 0..31), which the decoder keeps for the caller
  (`HnmVideo::alpha`) and `AlphaTables::blend` draws as the original's
  dialog video stream does: the frames are premultiplied, a = 31 copies
  the video, a = 0 keeps the background, and in between the background is
  scaled by (31 − a) / 32 with the original's tables.
- **Dialog heads** (`head.rs`, `raster3d.rs`, `raster3d/ii.rs`, `lipsync.rs`): a speaker's head is a `.db` scene written by Cryo's 3ds Max
  exporter (`crates/atlantis2-data/src/db.rs`: meshes, a node hierarchy
  with key-frame tracks, materials, the mouth shapes as morph targets,
  cameras) with a 512x512 16-bit RLE TGA texture and two eye-blink maps
  (`tga.rs`). The scene is posed at the body video's frame number, its
  mouth mixed from the mood's 7 visemes, and the head drawn by a
  transcription of the original's pipeline for the panorama's CPU (the
  same `Variant` as the warp, one reference machine). With `warpPII`, the
  default, it is the Pentium II's (the original's CPU type 8): the x87
  assembly world update, object → camera matrix, transform with its clip
  codes, back-face test (a face is drawn when N · c0 ≤ 0) and projection,
  and the rasteriser `Zi32Texture16FastPerspective1516II` (a triangle
  set-up chosen from the vertices' integer rows, the scan, and the MMX
  span: the texel coordinates every 32 pixels by a division, stepped in
  fixed point in between from their shifted ends, the last block's step
  from a float difference), its set-up loading 0x0e7f at each polygon.
  With `warpC` it is the C pipeline (`Zi32Texture1516FastPerspective1516C`:
  perspective every 32 pixels from truncated ends) under the process's
  word, which AMD CPUs drew. A fan of triangles, nearest texel, a 32-bit
  Z buffer, every float store and x87 rounding as the original's. A
  material group of the head mesh whose texture is not loaded is drawn
  flat black, with no Z test or write, by the routine `FlatC` (the C
  set-up and scan, a filling span; FUN_00429700 gives a material without
  texture black): only `Als1.db` (Ailill) has one, its `moustache` group
  naming `sonoir.tga`, which no disc has. The eyes
  blink at random (2 % a frame, then three frames closed, half closed,
  open), with the C runtime's own `rand`. The lip-sync analyses the voice
  by blocks of 256 samples (energy and a histogram of zero-crossing
  intervals) and turns it every frame into the 7 weights through the
  speaker's 10x7 `[Matrice]` of `scenar\info<world>.txt`, smoothed by
  `[Smooth]`, `[Offset]` seconds ahead of the voice (`dialog.rs` reads
  the scripts). Frames, Z buffers, intermediate buffers and the weights
  of every Tibet line are the original's, bit for bit: the heads with
  both variants (the Pentium II's under 0x0e7f, the C one under 0x027f;
  all 28 heads of the discs with the Pentium II's, `heads_pii_more.txt`;
  for each, three frames also step by step: the head's world pose, the
  camera, the camera-space and projected vertices, the clip codes and the
  face bytes), the lip-sync under 0x027f, 0x007f and 0x0e7f. One
  departure, on purpose: when a row's left edge is a NaN or beyond the
  32-bit range and its right edge is not, the original's II scan calls
  its span from x = −2³¹ (`fistp` gives the integer indefinite, and the
  test is only `xl < xe`), a write outside the picture; the port draws
  nothing on that row. No frame checked has such a row;
  `cargo run --release --example head_png` draws a head to a PNG (`c` as
  a sixth argument for the C routines).
- **The dialogs' control word:** the dialog's float code (the heads'
  load, pose, mouth and transform, the lip-sync) runs
  under the word the game's code has: after a `warpPII` frame 0x0e7f, as
  no code of the executable loads another between the panorama's frames
  and a dialog's (its only control-word loads are the assembly
  rasterisers' 0x0e7f and the C runtime's save-and-restore pairs; its
  math functions round to nearest inside, `Fpu::nearest`), and the II
  rasteriser loads it again at each polygon. The dialog takes the
  engine's word and hands back the one its heads leave. The numbers of a
  world's dialog scripts (`[Matrice]`, `[Smooth]`, `[Offset]`,
  `[Focale]`) are stored as floats under the word in force when the world
  is loaded (`monde`): toward zero under 0x0e7f, to nearest for a new
  game's Tibet, read before its first panorama frame. Not known: whether
  the original draws a panorama frame before a new game's first `monde`
  (the port does not), and what Direct3D, which the original calls only
  at start-up and when the display is set up, does to the word.
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
  break and place their lines as the original's code does (with 1.00, a
  byte with the high bit set is taken with the next one, so an accented
  letter before a space is no break; lines 12 pixels apart, in white over
  a black copy shifted by one pixel; 1.01 differs, see **Builds** above),
  with the widths of the game's own font,
  `loc\Atl_Font.ttf`, from its `hdmx` table at 16 pixels per em and its
  ascent from its `VDMX` table, as GDI uses them. The glyphs are the
  font's outlines filled where at least half covered: GDI also runs the
  font's hinting, which this does not, so a few pixels of a glyph differ
  from the original's (the one known difference). The voices are mixed as
  DirectSound mixes its buffers (gain 10^(attenuation / 2000)). A voiced
  line ends once its voice has been played out, DirectSound's status
  (FUN_0047a9c0(1) @ 0x0046101b): with a sound output, the mixer's (a
  voice the output has not played out a second after its length, a
  stalled device, ends by the clock; a window without a sound device
  plays the mix into nothing at the game's pace); without one (the
  tests), the voice's length on the clock. With
  `cinema=N,M` dialog M plays over film N: it is set up whatever the
  film's start answered (FUN_0040c030: a film that does not start leaves
  it to the next video), then at each video tick its line sequencer alone
  (no click, no camera change; its end is ignored) and FUN_00480840 over
  the film's frame: a `voixoff` line turns first, head and `voixoff`
  lines draw the panorama with its place animations' step over the film,
  every kind its dialog frame (no panorama for a `voixoff0` line there);
  the dialog removed when the film ends. The shipped film dialogs (71,
  168) have `cinema` lines only (`dialog_loop.txt`'s `film` rows, run by
  the original under emulation). Each frame of the dialog loop (the
  dispatcher's loop type 2 and FUN_00480840) does what the current line's
  kind asks: a head line and a `voixoff` line (which first runs the
  warp's FUN_0040c6a0: a move's pending turn steps on, its end left to
  the warp, else the mouse inertia within the view's limits; then the
  zone under the centre is picked again) draw the panorama as the warp does (FUN_0045a930: the
  place animations drawn and stepped on the 15 Hz ticks, so the place's
  characters, water and fire go on moving, and the web in its mode), then
  the dialog frame; a `voixoff0` line (Liam's off-screen lines in Ireland,
  the spider's and Tezcat's in Yucatán and Xibalba) draws the same
  panorama, then the dowsing rod and the overlays (FUN_00475320), then its
  subtitle, so the hero's head of a previous line does not stay on screen;
  a `cinema` line (only in the dialogs over films) draws its subtitle
  alone; no line, nothing. Checked against the dispatcher run under
  emulation with the line kind given (`golden/dialog_loop.txt`,
  `tests/common/dialog_loop.rs`, on lines of the four kinds: Tibet's
  dialog 360, Ireland's 1, 56, 44 and 71, Xibalba's 190): the panorama,
  its step and the turn, and the dowsing rod and the ring drawn (the rod
  stepped) under the dialog frame for a `voixoff0` line only. The
  dialog's end sets the previous loop type back, and that resets the warp
  camera (FUN_00402960: the screen's centre and 76°), whatever field of
  view the last line had: the panorama is not left zoomed in after a
  dialog (asserted after every dialog of the Tibet, Irish, Chinese,
  Yucatán, Xibalba and endgame tests, against the camera the original's
  code leaves under emulation). The lines, the
  video frames and their numbers, the composed frames, every subtitle's
  layout and the fades are the original's (tests/dialogs.rs).
- **Scripts:** the chapter scripts `scenar\chapiNNN.con` keep the first
  game's text language but not its interpreter: names come from
  `scenar\defnames.con` (five sections, looked up per key:
  `crates/atlantis2-data/src/defnames.rs`), compares are signed, a section
  has a head condition and sub-conditions, and the text is walked byte by
  byte with the original's quirks (a `#/set` line runs when the line
  after it holds no `/set`: an empty line, a `/con` or a section header,
  as chapter 19's line 74, which gives Liam's tree subject; it is skipped
  before a `/set` line or at the start of a section, checked against the
  original's scan in `tests/script.rs`; `KEY=++` stores 0, two kinds of
  line run twice in `/INIT`). The
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
  `anglewarpxnow/ynow` (the camera's rotation set about the vertical
  axis, the pitch dropped / a pitch from the current orientation, as
  emulated: Yucatán, below), the four zone keys (the type
  of the zones of that value in the first such view of the current place
  ± 88888), `ABORTDEPART`, the show / hide table and the animation keys on
  the view's slots (`hideperso`, `showperso`, `hideobjet`, `showobjet`,
  each on the first matching slot only,
  `PLAYANIMP` / `O`, `CONTINUEANIM`, `STOPANIM`, `GOTOFRAME*`,
  `ADDFRAME*`, `ChangeAnimGotoPerso`, and the `NUMFRAME*` conditions;
  only the first four occur in Tibet; Ireland adds `PLAYANIMP` / `O`,
  `CONTINUEANIM`, `ADDFRAMEOBJET` and `NUMFRAMEOBJET`; their effect on the
  slots, the id clash where the kind is not checked and the `NUMFRAME*`
  conditions are compared with the original's handler on synthetic slots,
  `scripts_worlds_anim.txt`, `tests/scripts_worlds.rs`; `GOTOFRAMEOBJET`
  past the file's count is fatal by its key), `cinema` (the film at 15
  frames a second with its sound track, the music at 20 %, dialog `M` over
  it), `dial` (the dialog loop; a failed start leaves the section waiting,
  as in the original; a `dial` while a dialog runs ends that one in full
  first, FUN_004808a0: `CLICPERSO` = 0, `SUJET` = −2 and the section that
  waited for it runs on from its wait before the new one starts), `startmusik`, `stopmusik`, `NEWSOUND`, `TrajSound`,
  `monde` (the disc rule, the objects bank, the dialog texts), the bag and
  the hand (`sacados` scrolls the bar to its end, `objetenmain=0` also
  hides the cursor), `ZoomStart` (the zoom of a move), `ViewBigSpr` (the
  picture viewer), `NextWarpInvalide` (the view entry skipped) and the
  special events (`TestSpecialEvent`; China above), `CREDITS` (the
  credits), `ZoomPsychoStart` (the zoom with its roll). `LOADMUSIK` and `delmusik` keep which music copies the original would
  hold and show the "please wait" page for a copy, the section waiting
  for its end (Menus, below); `monde` asks for the world's disc (the CD
  page when it was not found); `CHANGECD=n` asks for disc n − 1 as `monde`
  does (FUN_0040dff0(n − 1, 1)); `MESSAGE_CLIC=<id>` shows `loc\Messages.txt`'s
  line for the id over the panorama, which stops turning (loop type 10,
  `message.rs`: in `AtlFT_Big_Default` at the centre, a text wider than
  640 pixels cut at half its bytes and only its second half shown, an
  unknown id as `Unknown message ID :` and the id; a click or two seconds
  end it; the text's colour is 0x0fffffff, as the warp's `PAUSED`
  overlay's, which Windows 98 SE's GDI draws white and Windows 11's black
  (measured on 16-bit DIB and DirectDraw surface DCs): the port draws it
  white, as on the Windows 9x of the game's release); no shipped script
  uses either.
  `SHOWGUEST`, `INCPERSO`, `ADDGUEST`, `SUBGUEST` and the plain
  variables are script state only, as in the original. `ANIM_THE_FRAME`
  sets the playing byte of the first record of its id, of any kind
  (@ 0x00483371..0x0048339e; `tests/anim_the_frame.rs`). A fatal error of
  the scripts or of a puzzle (an unknown name, a missing chapter, a slot
  or an animation the data lack, the view a deferred place change names
  missing in the new file) runs the original's reporter (FUN_00480cd0): with a view
  entered, the emergency save of player 9, slot 8 (`save\ATL09_08.SAV`,
  which the menu never lists), then the game quits; the shipped data
  never reach one. A dialog number with no line plays, in the original,
  one `voixoff` line of a speaker `Dummy` whose id is the error message
  (@ 0x0045ebc0, FUN_00460830 with stand and camera −1, mood 1, kind 1
  [the arguments' roles inferred from the line parser's own call]);
  here the dialog is refused (no voice, head or text could be found for
  it, and every number the shipped scripts and films start has lines,
  `tests/*` "every dialog plays"). The original's own unknowns stay open:
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
  line when short; a missing file is fatal in the original's
  `AtlMenu_MainInit`, @ 0x00486298, while the port logs a warning and
  shows the menus without texts: no shipped release lacks it). The widget classes are the original's: sprites,
  buttons (the box frame 0's size centred on the position, strict
  inequalities), hover buttons (frame 1 under the mouse), animated
  buttons (20 frames, one per 15 Hz tick while the mouse is in the box
  with the button up, while pressed by a click in the box until the
  button is released, or always), two-state and blinking toggles, the volume sliders (gauge and
  knob, x0 = x − 65, 115 pixels, the action every frame while held) and
  the load page's list (60-pixel rows, the thumbnail of `save.spr` and the
  date and time in `AtlFT_POPUP`, newest first) with its scroll bar (at
  the list's right edge, a 200-pixel track); the date and time are the
  front end's (`Engine::date_text`): on Windows the user's locale's short
  date and time, as the original's `GetDateFormatA` / `GetTimeFormatA`,
  elsewhere a French system's dd/mm/yyyy hh:mm:ss. A tooltip shows after
  10 ticks in a box, centred on it in white over black; its count restarts
  when the mouse enters the box of an enabled widget, and a disabled
  widget still notes whether the mouse is in its box, so once enabled it
  keeps its count (FUN_0046d750, against the original run under
  emulation, `tests/input_ui.rs`). Each frame the
  widgets are asked in order and the first action wins; the state machine
  then slides the main page's panels (1 + 2 + ... pixels a frame: 30
  moves for the options and the disc, 40 for the sound, the state
  changing at the frame after the last move, as the original's; the disc
  panel's load and save icons are shown, disabled, from the slide's first
  frame and enabled then; NO to the save question enables every widget
  again before the slide back), asks its
  questions in the yes / no box (save, erase, quit; the box and its two
  buttons enabled), opens the pages, and
  sets the options (subtitles, centre marker, inverted mouse, the screen
  size: 640 x 480 or 800 x 600, kept in `atlant2.cfg`, below) and the volumes (general = slider × 256, music = music slider
  × general slider × 127 + 0.5, effects and dialogs × 127, truncated);
  at each opening the sliders start at general × 1/256, music ×
  0.007874016 / general (0 when general is 0), effects and dialogs ×
  0.007874016 (floats, under the control word; the whole table checked
  against the original's, `tests/ui_fixes.rs`).
  Escape closes the main menu opened from the game (not the player menu).
  A menu keeps the panorama's turn speeds: only the panorama frame
  decays them, and the original's motion callback adds every mouse move
  to them in every loop type, so a move of the pointer in the menu
  swings the view when it closes (the port adds the system pointer's
  moves in screen pixels, the original the mouse's counts). The
  button-held flag that keeps the animated buttons still is set by every
  press and cleared by every release, in every loop type.
  The cursor is `curseur.spr` at the mouse; the window then lets the
  pointer go and draws the game's own. Quitting shows the credits, as
  does the scripts' `CREDITS` (`crates/atlantis2-engine/src/credits.rs`,
  checked frame by frame against the original under emulation,
  `tests/credits.rs`): the pages of `loc\Credits.txt` (21 in the French
  release; a missing file is fatal in the original, @ 0x00470c63, while
  the port logs a warning and the credits end at once) over the 16 pictures `f_cred01.tga` .. `f_cred16.tga` in turn,
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
  differ). The media pages (`engine/media.rs`): "please wait" is the
  original's music copy (`CopyMusikToDrive` @ 0x00479480: each music of
  the list whose hard-disk copy is empty is copied from the current disc,
  0x40000 bytes at a time with a frame of the page after each read and
  each write; one not on the disc is not copied). The port copies
  nothing; it keeps which copies the original would hold
  (`music_copies.txt` beside the saves, from session to session, as the
  original's files, the first session starting from the install's empty
  ones; `delmusik` deletes them; without a data folder, in the tests and
  tools, every copy is taken as made until a `delmusik`) and shows the page for the
  frames the original draws, two per block of the `.apc` (46 for
  `muzik30` at a new game): the time a real drive takes over each block
  is not modelled. The game is held meanwhile, as the original's copy
  loop runs nothing else: the section that ran `LOADMUSIK` or the
  `startmusik` (or the load) waits for the page's end, no 15 Hz tick
  runs (the copy loop calls no scheduler: the scheduler's pass that
  started it goes on after its last frame, a tick due in that pass runs
  then and the next one is counted from that pass's time, so it is due
  in the first pass after the page), the music machine runs once a frame (the menu frame's call), and
  the mouse's and the keys' events wait until the end (DirectInput's
  buffer). Checked against the original's copy under emulation
  (`tests/ui_fixes.rs`). The CD page (FUN_0040dff0) asks for a world's
  disc no drive holds; the port reads every disc it found as if each
  were in a drive of its own, so it asks only for a disc that was not
  found, polls the folders the discs were looked for in every 101 ticks
  (state 9: `100 < ticks − start`) and goes on when the disc is there.
  "Disk full" and "write error" (FUN_00486c60, FUN_00486d10) are opened
  only by the copy, when the free space is short or a block's read or
  write is short; the port writes no copy, so they never open. Not known, and
  chosen here: whether the page title (line 0 of each section) is drawn
  (it is not). At 800 x 600 the pages are drawn at 640 x 480 and
  enlarged, as in the original (below). The list's date and
  time are written in the French system's formats (`22/10/2026
  14:12:00`); the original asks Windows for the user's.
- **Screen sizes** (`crates/atlantis2-engine/src/display.rs`,
  `engine/display.rs`): the original's options page offers 640 x 480 and
  800 x 600 (its button, action 14, `FUN_00402de0`
  `AtlDisplay_SetNewScreenSize`; kept in `atlant2.cfg`), and the port has
  both, as the original's own mode (not an enhancement). At 800 x 600 the
  original keeps two surfaces, the screen and a 640 x 480 one
  (`FUN_00402720`), and each loop type change (`FUN_0049da90` →
  `FUN_00402960`) chooses where the frame is drawn and the size the code
  lays things out for (@ 0x005d9b9c, 0x005d9ba0). The panorama (and the
  `MESSAGE_CLIC` text) is drawn on the screen itself: the cube
  at 800 x 600 with the same 76° across (fx = 800 / (2 tan 38°) = 511.98,
  centre (400, 300); 60.7° high, as at 640 x 480), the cursor and the
  object in hand at its centre, the bag bar with 10 cells and its art
  twice (`FUN_00475740`, `FUN_00474c30`), the overlays and the dowsing
  rod placed by the screen's size, the subject menu at its fixed
  positions. Every other loop type (the films and transition videos, the
  dialogs, the menu pages, the credits, `ViewBigSpr`'s pictures) is drawn
  on the 640 x 480 surface exactly as at 640 x 480, then enlarged onto the
  screen by the original's MMX routine (`FUN_00401202` for RGB565, called
  by `AtlDisplay_AutoBlit` `FUN_00402c30`): 4 pixels to 5 across and 4
  rows to 5 down, each channel mixed 1:3, 2:2 and 3:1 with truncating
  shifts, the first and last copied (`blit_640_to_800`, checked against
  the routine run under emulation on four synthetic pictures:
  `tests/display.rs`, `golden/blit800.txt`). A dialog is drawn on the
  screen itself only when it is one of eleven (`FUN_00480110`: 44, 82..88,
  187..189, single off-screen voice lines over the panorama in Ireland and
  Yucatán). The dialogs' field of view takes the screen's width
  (`FUN_004801d0`, `FUN_004808a0`), not the surface's, so behind a head at
  800 x 600 the panorama, drawn on the 640 x 480 surface, is 1.25 times
  narrower than at 640 x 480: the original's way (the head, in its own
  640 x 480 viewport, is the same). At 800 wide the menus' dimmed backdrop
  is black (`FUN_00470430`). Each loop type change sets the camera again
  (centre, 76° for the size drawn, under the x87 control word of the
  moment), at 640 x 480 too, as the original does on every path of
  `FUN_00402960`: after a dialog that ends on a line with a `[Focale]`
  field of view, the panorama is back at 76° (checked against the
  camera the original leaves under emulation, `golden/dialog_loop.txt`).
  The surface changes between passes (a change in the
  middle of a pass takes effect at the next one; a film started by a
  panorama frame has its first frame enlarged at once). The window
  scales the 800 x 600 picture as it scales the 640 x 480 one, and a
  window pixel maps to the pixel of the size the frame is laid out for.
  The HD mode keeps 640 x 480 (its picture is drawn from the 640 x 480
  screen); the choice is kept for when it is off. `--resolution
  800x600` chooses it from the command line (as the button, so it is
  written to `atlant2.cfg`). Timings at 1920x1080 (release, i7-9700K;
  `--size 1920x1080 --headless`): a panorama frame's pass 1.6 ms (1.1 ms at
  640 x 480), a dialog's 4.4 ms (4.1 ms), a menu's 0.9 ms (0.6 ms), plus
  1.2 to 1.4 ms to scale the picture to the window.
- **Options menu and settings** (`crates/atlantis2-engine/src/options_menu.rs`,
  `settings.rs`; the window's side in `crates/atlantis-app/src/a2.rs`):
  F10 (or O) opens the first game's options menu: its box, font,
  colours, keys and mouse are the same code
  (`atlantis_engine::options_menu::MenuBox`, which the first game's menu
  is drawn with too), Atlantis II giving its own rows: Graphics (Original or HD), the HD options
  (panorama filter, sharpening, interface, sharp subtitles, widescreen, the bars beside the
  videos: black or blurred; greyed in the original graphics), Screen (640
  x 480 or 800 x 600, the original's option above; greyed in HD), Fix
  original bugs, Arrow keys turn (`ui_ux`), Close. The game is paused while it is open (its clock
  and its sound); its text is drawn sharp in the HD mode, as the first
  game's. The choices are written at once to Atlantis II's own
  `settings.ini`, beside its saves and `atlant2.cfg`, with the first
  game's keys (`minor_bug_fixes`, `ui_ux`, `graphics = original|hd`,
  `hd_panorama`, `hd_sharpen`, `hd_interface`, `hd_sharp_subtitles`,
  `hd_widescreen`, `hd_video_sides = black|blur`); the screen size goes to `atlant2.cfg`,
  written then too (the original writes it at exit). A window reads
  `settings.ini` at start (written with the defaults when there is none);
  `--hd`, `--widescreen` and `--original` (which turns the bug fixes and
  `ui_ux` off)
  hold for the run without changing it; a headless run starts from the
  defaults. `--options-menu` draws the menu over a headless run's last
  frame. The web build does not run Atlantis II yet, so it has no such
  menu for it.
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
  `/INIT` again. The skull's state (entry 30), the ogham stones' bytes
  (entry 31) and Brigid's cross (entry 32, which the engine reads and
  writes in place) are the engine's, and so is the spider web's stage
  (entry 27, @ 0x005e1340, initial 3: saved from the web's state and set
  back before the web's load hook tests it); the other worlds' puzzles
  (entries 20..26, 28, 29) are kept
  as the bytes of the last load until their studies are ported; the rod's
  entry 15 (@ 0x006d84f4) is its anchor's y, the logical height, which
  the rod's draw writes at every panorama frame and reads right after (480
  from the start-up, 600 in the panorama at 800 x 600) (the
  blackbird songs have no entry: their mode is not saved); the dialog flag is written from the
  running dialog and not read back (a save made from the menus has no
  dialog running: Escape ends it before the main menu opens). A load's
  puzzle reset runs the original's eight quits (the turtle ring's and the
  souls' modes back to 0; the canoe's, which has no test of its own,
  empties the cursor pair, so the saved cursor frame is not restored;
  its change of the display mode is not ported: at 800 x 600, after a
  menu opened over a picture and a load, the original draws the picture
  at the top left of the screen until the next loop type change; the
  rest of it changes nothing at a load, which never happens during the
  canoe (Escape does nothing in its loop type, so no menu opens) and the
  canoe always ends through that quit: its banks, 3D slots and depth
  tiles are already freed, its board flag @ 0x006d8278 already 0, the
  3D library already emptied (by the canoe's last quit or the web's free
  @ 0x00491241, which the web's quit in the same reset runs), the
  secondary data folder already none (set and reset within the web's and
  the canoe's loads), the 3D draw's Z surface pointer @ 0x00659bd8 is set
  by each drawing caller before it draws (@ 0x00491b60, 0x004978dc,
  0x00499a40, 0x00499a69), and FUN_0042a720(800, 600) makes the 3D Z
  surfaces again at the size they have outside the canoe, the configured
  one (made so by the start-up's FUN_0042a6e0 @ 0x0049d71e, the flag
  @ 0x005d9ba4 still its initial 1, by the screen size option, and by
  every canoe quit); the 1.01 executable's quit is the same,
  @ 0x0049a5d0). Chosen here: a section
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
copies), `ATLANTIS2_GOG` GOG.com's installation (only read; `tests/gog.rs`:
the folder, the web's and the canoe's textures, a new game to the shaman's
dialogs with 1.01's subtitles, a save and a load, dialog 398 over film 66;
the 1.01 subtitles in `tests/dialogs.rs`, the listener table in
`tests/place_sounds.rs`, and `tests/flow.rs`'s whole game with the mouse
played again on it, its pictures written as `gog-*.png` with
`ATLANTIS2_PNG_DIR`), `ATLANTIS2_PATCH101` the 1.01 patch's
`atlantis2.exe` (`tests/gog.rs` puts it beside links to
`ATLANTIS2_DATA`'s folders: a CD installation patched to 1.01). An
Atlantis II test whose data is missing fails, saying which variable it
needs, unless `ATLANTIS2_SKIP_DATA_TESTS=1` asks for the game-data tests
to be skipped (the CI's builds set it); a skipped test writes a
`SKIPPED <test>: <what is missing>` line that `cargo test` shows
(`atlantis2_data::testing`). With `ATLANTIS2_PNG_DIR` set, `tests/skull.rs`'s and
`tests/ogham.rs`'s played games write their screens there
(`pz-skull-*.png`, `pz-ogham-*.png`), and with `ATLANTIS2_SHOTS` set
`tests/cross.rs`'s and the Chinese puzzles' (`tests/figurines.rs`,
`chest.rs`, `wheels.rs`, `dragon.rs`, `turtle.rs`, `labyrinth.rs`:
`pz-cross-*.png`, `pz-figurines-*.png`, `pz-chest-*.png`,
`pz-wheels-*.png`, `pz-dragon-*.png`, `pz-turtle-*.png`,
`pz-labyrinth-*.png`) and `tests/xibalba.rs`'s canoe (`pz-canoe-*.png`); the expected frames, matrices, zone picks, heads, lip-sync
weights, dialog lines, dialog frames, subtitle layouts, music fades,
script results and save files come from the original `atlantis2.exe` run under emulation
(Tibet's moves pass by pass, the place animations tick by tick, the
screen centre and the clicks on sprites pass by pass with the texel under
the centre given, every SPW frame of Tibet and the first of every other
file, every `AFF_SPR` and `AFF_SPRTRS50` of the installed banks with
`spr_p5.dll` and `spr_p6.dll`, the
subject menu's cells and click grid, the shaman's subjects along chapter
75, every `/set` line of Ireland's, Yucatán's and China's chapters
through the handler, the special events, the zoom of a move,
Brigid's cross's hooks, frame tables and pictures, the skull puzzle's
hooks, frame lists, clicks with chapter 7 and drawing, the ogham stones'
moves and drawing, the blackbird songs' draws, clicks and warp frames,
the figurines' slots, hooks, check, dispatcher, warp frame with chapter
47, step and drawing, the Chinese chest's hooks, dispatchers, clicks with chapter 47 and
drawing, the five wheels' hooks, check, zone clicks with chapter 55 and
drawing, the pearl and dragon's hooks, steps, frames at five tick rates,
clicks with chapter 58 and drawing, the turtle ring's hooks, tick,
dispatchers, clicks and arrivals with chapter 46 and drawing, the
labyrinth souls' cells, visibility tests, random drops, entry, delay,
tick, warp frames with chapter 60, dispatchers and drawing, the canoe's
loading and every frame of a played canoe, under both CPU routines,
every frame of the 1086 `.hnm` files of the four discs (the videos
decoded over surfaces filled with 0xa5a5 rather than zeroed, as the
original never clears them, with the same frames; each video ends on
its frame count, a size-0 dword right after its last counted
superchunk), the dialog videos'
blend with the RGB565 and RGB555 tables at 640 and 800 pixels a row,
the 161 sound files decoded whole, each music's start;
the credits' text widths are checked against GDI's widths of the font
measured on Windows 98 SE and Windows 11;
names of `defnames.con`
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
| next | the other worlds: Ireland (its places, chapters, dialogs, place sounds, transition sprites, its four puzzles, the way back to Tibet; done), China without its puzzles (its places and links, chapters, dialogs, place sounds, the zoom of a move, the picture viewer, the special events, the way back; done), China's seven puzzles (done), Yucatán with its grids (done), Xibalba and its three puzzles (done), the space-travel hub, Shamballa and Atlantis without their two puzzles, the end of the game (done), the fireflies and the octopus fight (done): the whole game plays |

