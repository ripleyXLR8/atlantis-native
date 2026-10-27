# Atlantis Native: player's guide

[Back to the README](../README.md)

## Getting the game files

Atlantis Native is a new engine: it does not include the game. The
pictures, videos, voices and music of *Atlantis: The Lost Tales* belong
to its publisher, so Atlantis Native plays them from your own copy. It
only reads the files and never changes them; it never runs the game's
programs.

On its first start it looks for the game on your computer (the GOG.com
installation, a copy in a usual place) and offers it; otherwise it
explains what it needs and asks for the folder. It remembers the folder
in its [game library](#several-editions-the-game-library).

### From GOG.com

Install the game with GOG Galaxy or the offline installer, then choose
the folder it was installed to (for example
`C:\GOG Games\Atlantis The Lost Tales`). The installer itself (a
`setup_atlantis_...exe` file) is not enough: run it first. On macOS,
choose the game's app: Atlantis Native looks inside it.

### From the CD

The CD release spreads its files over four discs, one archive each:
`BIGCD1.BIG` on disc 1, `BIGCD2.BIG` on disc 2, and so on. Make a folder
and copy into it:

- `atlantis.exe` from disc 1;
- `BIGCD1.BIG`, `BIGCD2.BIG`, `BIGCD3.BIG` and `BIGCD4.BIG`, one from
  each disc.

Then choose that folder. A folder the CD's installer made
(`C:\Cryo\Atlantis`, for example) is not enough on its own: the
original game reads the archives from the disc, so copy them into it.

Simpler: put disc 1 in the drive when Atlantis Native starts for the
first time (or choose the drive in the folder picker). It offers to copy
the game from the four discs itself: it asks for each disc in turn and
copies its files (about 2.3 GB in all, a minute or two per disc) into
the `game-files` folder of its per-user folder, then plays from there
without the discs. A copy cut short goes on where it stopped.

### From images of the CDs

Atlantis Native reads the game straight from images of its four CDs:
put the images in one folder and choose it (the image file names do not
matter). It reads `.iso` images, raw `.bin` images (with or without
their `.cue`, as Redump's), `.img`, `.mdf` and Nero `.nrg` images.
Compressed images (`.zip`, `.7z`, `.chd`) must be extracted first.

### Supported editions

The English CD (1997 and 2001 builds), the French CD (May and August
1997), the German CD (August 1997), the Dutch and Swedish CDs (June
1997), the Spanish CD (August 1997), the Italian CD (July 1997), the CD
with Cryo's patch 1.01 (over the English, French, German, Dutch,
Swedish, Spanish or Italian CD), the GOG.com release in
English, French, German, Spanish and Italian, and the Traditional
Chinese CD (1998) and the Simplified Chinese CD (1999), copied or as
images. Another edition is
refused with the size of its `atlantis.exe`, and the message offers to
report it in one click: **Report it on GitHub** opens a new issue on the
project's page in your browser, filled in with the identifiers of the
files (the size, date and checksums of `atlantis.exe`, the size and
checksum of the start of each `BIGCD` archive, the names of the files in
the folder; never the folder's path). Add where your copy comes from and
submit it: nothing is sent before. The web build shows the same report
as a link.

### When the folder is refused

The message says what is wrong and what to do:

| Message | What to do |
|---|---|
| holds the game, but not all of it | copy the `BIGCD` files it lists, from the discs it names |
| holds atlantis.exe but none of the game's archives | copy `BIGCD1.BIG` to `BIGCD4.BIG` from the four discs |
| holds game archives but not atlantis.exe | copy `atlantis.exe` from disc 1 |
| holds disc images that Atlantis Native cannot read | extract compressed images, or copy the files out of them |
| the disc images hold the game, but not all of its four discs | add the images of the discs it lists |
| holds the GOG.com installer | run the installer, then choose the installed folder |
| holds an atlantis.exe that Atlantis Native does not know | an edition not supported yet (see above) |
| does not hold Atlantis: The Lost Tales | choose the folder that holds `atlantis.exe` |

Choosing the folder above the game's works too: Atlantis Native looks a
few levels down for `atlantis.exe`.

## Several editions: the game library

Atlantis Native keeps a list of the game folders you have played: the
game library. With one game in it, the game starts at once, as before.
With two or more (the GOG.com release and the CD, the English and the
French CD...), the program opens on the library's screen to choose the
one to play, the last played selected. Each game is shown as its
edition, *GOG.com, French (1999 build)* or *CD, English (May 1997
build)* (with *from disc images* when it is read from images of the
CDs), and its folder.

| Action | Mouse | Keyboard |
|---|---|---|
| Choose a game | click it | Up / Down |
| Play it | double-click it, or **Play** | Enter |
| Add a game folder | **Add a game folder...** (the folder picker) | A |
| Find the installations on this computer | **Find installations** | F |
| Take a game off the list (no file is deleted) | **Remove** | Delete |
| Quit | **Quit** | Escape |

**Find installations** looks where the first start looks (GOG.com's
installation, the usual places, the mounted discs and disc images, the
copy made from the discs), adds the games not listed yet and says how
many it found. A folder that no longer holds the game (moved, renamed,
on a drive not plugged in) is shown in red as *not found*: **Locate...**
(in place of Play) asks where it is now, or remove it.

To open the library's screen with one game in it: hold **Shift** while
the program starts, or start it with `--launcher`. While you play, the
last entry of the options menu (F10), **Game library...**, stops the
game and goes back to the screen to play another edition (the progress
since the last checkpoint is not kept, as when quitting). The settings,
players and checkpoints are kept per build of the game (see
[Players, checkpoints and settings](#players-checkpoints-and-settings)).

The list is the file `games.txt` in the per-user folder, one game folder
per line, the last played first; it can be edited by hand while the
program is closed. (Earlier versions kept one folder in
`game-folder.txt`: it is taken over.) A game folder given on the command
line is played without the library and not added to it.

## The support screen

Now and then, at start-up, before the game library or the game, a small
screen thanks you for playing, with links to the project's
[Buy Me a Coffee](https://buymeacoffee.com/ripleyxlr8) page and its
[GitHub page](https://github.com/ripleyXLR8/atlantis-native) (a star
there helps too). **Let's play!**, Escape or Enter goes on to the game;
opening a link keeps the screen open. Tab or Left / Right choose a button
or the checkbox, Space ticks it.

It shows rarely: never on the first launch, only once the program has
been started at least 3 times and games have run for 2 hours in all (the
time in the F10 menu does not count), and then at most once every 7
days. A run started with a game folder or a testing option (`--headless`,
`--puzzle`, `--new-game`, `--support-screen`) neither shows it nor
counts: its launch and its play time are left out. Nor does it show in
the web build.

To turn it off for good, tick **Don't show this again** before closing
it, or set `never = on` in `support.ini` in the per-user folder (see
[Players, checkpoints and settings](#players-checkpoints-and-settings));
the file also holds the counts it goes by (`launches`, `played_seconds`,
`last_shown`). `atlantis-native --support-screen` shows it at once,
whatever the counts say.

## Full screen

F, F11 or Alt+Enter switches full screen on and off (desktop and browser;
F not while a game menu takes typing, such as a player's name); the web
page also has a **Full screen** button at its top right.

## Looking around with the keyboard

In the panoramic view, the arrow keys turn (← →) and tilt (↑ ↓) the view,
as the original's F5 / F6 and F7 / F8 do.

## The options menu

F10 (or O, for a keyboard without function keys; not while a game menu
takes letters) opens the port's options menu, in the desktop and the web
builds; the game waits while it is open. Arrows and Enter (or the mouse)
change an entry; Escape, F10 or O close it. Its entries:

- **Graphics**: *Original* or *HD* (see [HD mode](TECHNICAL.md#hd-mode)),
  and with HD on its picture options (**Panorama filter**, **Panorama
  sharpening**, **Interface**, **Sharp subtitles**, **Widescreen**,
  **Video sides**).
- **Fix original bugs**: *On* (every fix, the default), *Off* (none: as
  the original), or *Custom (n/N)* when some of the game's fixes are on
  and some off; Enter turns them all on, then all off (the game's fixes
  only: a switch the edition does not list keeps its value, for another
  edition that shares its settings).
- **Choose the bug fixes...**: opens a box of its own, in the same
  style, named after the game played, with *All on*, *All off (as the
  original)*, one switch per fix and *Back*; the foot of the box tells
  what the original does without the fix selected and where a player
  notices it. Arrows, Enter or a click change a fix; Escape (or a click
  outside the box) goes back to the options, F10 or O closes everything.
  The box lists only the fixes of the edition played. Those of every
  edition:
  - **Music: seamless loops**: the original streams the music through a
    ring its refills do not always keep ahead of, and stale fragments of
    sound are heard in it.
  - **Faces: the guards' coif, the pilot's scarf**: the original draws
    the overlay of these talking heads where the next copy of the
    picture erases it, so it is never seen.
  - **Faces: the lower face's first frame**: on the first frame of each
    loop of some talking heads (Meljanz, Actyon, Servage, the Queen,
    Gimbas) the original draws the 3D lower face away from the face.
  - **Inventory: icons left as the bar hides**: with the inventory bar
    scrolled, the original leaves rows of icons at the bottom of the
    screen as the bar slides away.
  - **Inventory: a new object's icon**: after a pick-up the original
    draws the new object's icon 7 pixels left of its slot until the bar
    next slides in.
  - **Menus: each its own click areas**: the original keeps one table of
    the menus' click areas for the whole session: the LOAD list's
    scrollbar knob, once hidden by a visit with 11 checkpoints or fewer,
    stays hidden, and on the May 1997 English and French CDs the ARE YOU
    SURE ? prompt loses its left ornament after the sound options.
  - **Players: OK on the fifth player**: with five players, OK on the
    fifth empties its name in the original, and that player can no
    longer be chosen.
  - **Snakes puzzle: the false first win**: on a first visit to the four
    snakes puzzle, the original takes the first piece carried to the
    screen's top left corner for the solution.

  Those of some editions:
  - **Hexagon puzzle: its pieces always there** (the 1997 and Chinese
    CDs): those builds place the hexagon puzzle's pieces only when the
    player comes holding the right object; the later builds always place
    them.
  - **Music: tracks copied one by one** (the May 1997 French CD): that
    build copies a music list to the disk all or nothing, on its first
    track, where the others copy each missing track.
  - **German: the sharp s in the LOAD list** (the games whose texts
    hold `ß`: the German CD, also with the patch 1.01 over it): no
    sprite font of the game has `ß`; the original leaves a gap in the
    LOAD list's chapter names, the fix draws it as "ss".
  - **Dutch: the credits and fonts of disc 1** (the games whose discs
    hold different copies of them: the Dutch CD, also with the patch
    1.01 over it): the original reads the credits and the name fonts
    from the disc in the drive, and disc 2 holds an older revision.
  - **Italian: the whole of OK and Annulla** (Italian CD): the original's
    click areas of the menu lines at a fixed place stop short of the
    drawn word, and the right ends of `OK` and `Annulla` do not answer.
  - **Chinese: the subtitle rows** (Simplified Chinese CD): the original
    may start a subtitle row in the middle of a character (after an
    ideographic space or a mark), and that row's characters come out
    wrong (72 rows in 50 lines).
  - **Chinese: the SUBTITLES button** (Chinese CDs): with SUBTITLES off
    the original reads that button's record one byte out of step, and its
    click area takes a wrong width and height.
- **Restored content** (off by default) and **Modern UI/UX** (off by
  default): see [Enhancements](TECHNICAL.md#enhancements).
- **Buy me a coffee...**, **Close** and, on the desktop, **Game
  library...**.

The choices are kept at once in `settings.ini` (see [Players,
checkpoints and settings](#players-checkpoints-and-settings)), one line
per fix. `--original` turns every fix and the other enhancements off for
that run only: a change in the menu during such a run writes only what
it changed.

## Atlantis II

Atlantis II plays from its start to its end (Tibet, Ireland, China,
Yucatán with Xibalba, the space travel, Shamballa with its fireflies,
Atlantis with the octopus fight, the credits;
see [Atlantis II](TECHNICAL.md#atlantis-ii-in-progress)). For now only the
command line opens it: give the folder of the installed game (the one
holding `atlantis2.exe`, `wam`, `scenar`...), and the folder of the CD
images or copies of the CDs if they are not inside it:

    atlantis-native --discs /path/to/cd-images /path/to/Atlantis_2_Files

The GOG.com edition needs nothing more: give its installation folder (the
one holding `Game.exe`, for example `C:\Program Files\GOG
Galaxy\Games\Atlantis 2` or `C:\GOG Games\Atlantis 2`). It holds every
disc, so no disc is asked for.

    atlantis-native "C:\Program Files\GOG Galaxy\Games\Atlantis 2"

GOG's edition is the game patched to version 1.01, and Atlantis Native
plays it as 1.01 does: the subtitles' lines further apart (they no
longer overlap) and broken after accented letters too, and no Alt+Enter
(F11 switches full screen with every version). A CD installation patched
to 1.01 (the patch's `atlantis2.exe` copied over the CD's) is played the
same way.

The game opens in full screen, as the original does: its options file
`atlant2.cfg` says full screen until the choice is changed. F11 switches
between full screen and a window, and the choice is kept there for the
next start (`--fullscreen` opens in full screen whatever it says).

### Controls

| Input | In the game |
|---|---|
| Mouse (and the arrow keys, an option of the port) | turns the view; the cursor at the centre of the screen tells what is there |
| Left click | walks on (a move cursor), takes an object, uses the object in hand, opens a character's subjects; in a dialog, the next line; ends a film (a double click ends them all at start-up) |
| Right click | opens or closes the bag bar at the bottom (the mouse then moves a pointer over it; a click puts an object in the hand) |
| Escape | the game's menu (options, volumes, load and save, quit), and closes it; ends a dialog |
| F10 or O | the port's options menu (below); the game waits while it is open |
| F11 | full screen on and off, kept for the next start (the left Alt with Enter too with the CD's version 1.00, as in the original; version 1.01 removed it) |

Xibalba's canoe puzzle has a screen of its own, with a pointer: a left
click takes a river tile, lays it back or swaps it with the clicked
square's, and on the landing stage starts the canoe; a right click turns
the held or pointed tile a quarter; Escape leaves the puzzle. On the
spider's web the view turns as usual and a left click on a way moves
along it once the spiders stand still; with the feather in hand the click
lays it there, and a click on a spider asks about it (or strikes it with
the sword). In Atlantis the centre shows a crosshair; in the octopus's
view, a tentacle that strikes is shot by a click on it, but only with
the sunken crystal in the bag (six shots a tentacle); a strike not shot
in time lands, crystal or not, and the blows wear the hero down.

In the game's menus the mouse moves the game's own pointer. As in the
original there are no named saves: the menu's disc saves in the next free
slot of the current player (99 each) and loads any of them.

### Options and the HD mode

F10 (or O) opens the port's options menu, the same as the
[first game's](#the-options-menu):
arrows and Enter (or the mouse) change an entry, Escape, F10 or O close
it. Its entries:

- **Graphics**: *Original* (the original's picture, scaled to the
  window) or *HD*: the panorama and the dialogs' 3D heads drawn at the
  window's resolution with the original's own projection (see
  [Atlantis II](TECHNICAL.md#atlantis-ii-in-progress), item HD mode);
  the subtitles, the menus' tooltips, the bag's names and the credits'
  texts drawn sharp from the game's font at that size, with the
  original's line breaks; the cursor, the bag bar, the menu pictures, the
  dialogs' filmed bodies and the videos are the original's 640x480
  pictures, smoothly enlarged (they cannot be made sharper). Off
  by default. The panorama is drawn finer as soon as the window's picture
  is wider than 640 pixels (in the window's default size, 1280x960, at
  twice the original's resolution), and the larger the window (or full
  screen, F11), the finer it is.
- With HD on: **Panorama filter** (Bicubic, sharper, or Bilinear),
  **Panorama sharpening**, **Interface** (Smooth or Pixels),
  **Sharp subtitles** (the subtitles and the menus' texts drawn at the
  window's resolution; on by default), **Widescreen** (in a window wider than 4:3 the panorama fills the
  sides too) and **Video sides** (black or blurred bars beside the
  videos in widescreen).
- **Screen**: the original's two screen sizes, 640 x 480 and 800 x 600
  (below); with the original graphics only.
- **Fix original bugs**: *On* (every fix, the default), *Off* (none:
  as the original), or *Custom (n/4)* when some are on and some off;
  Enter turns them all on, then all off.
- **Choose the bug fixes...**: opens a box of its own, in the same
  style, with *All on*, *All off (as the original)*, one switch per fix
  and *Back*; the foot of the box tells what the original does without
  the fix selected and where a player notices it. Arrows, Enter or a
  click change a fix; Escape (or a click outside the box) goes back to
  the options, F10 or O closes everything. The fixes:
  - **Music: every track heard to its end**: the original starts the
    next track about 6 seconds before the old one ends, so the last
    seconds of each piece of music are never heard.
  - **Music: an ended track starts again**: when a track ends with
    nothing to follow it, the original repeats its last 6 seconds over
    and over until another music starts; with the fix the track plays
    again from its start.
  - **Music: the rare missing mark (1 in 32768)**: once in 32768 random
    starts of a music, the original picks a place past the track's last
    mark and stops with a fatal error (it saves and quits); with the fix the
    track plays from its last place.
  - **Ireland: cross and ogham clicks**: a click on Brigid's cross or on
    an ogham stone works the puzzle and nothing else, where the original
    may take it for a click on another place's object: back to Tibet
    from the fourth stone in one chapter, the water's film from the
    ninth with the horse in another.
- **Arrow keys turn**: on by default, the arrow keys turn the view as
  the mouse does; off, as in the original, only the mouse turns it. On,
  the click that brings the window back to the front only takes the
  mouse back and does not act in the game.

The choices are kept at once in `settings.ini`, in the `atlantis2`
folder of the per-user data folder (`~/.local/share/atlantis-native/atlantis2`
on Linux, `%APPDATA%\atlantis-native\atlantis2` on Windows,
`~/Library/Application Support/atlantis-native/atlantis2` on macOS),
beside the saves (`save\ATL<player>_<slot>.SAV`, the original's files)
and the original's options file `atlant2.cfg`. `settings.ini` has one
line per fix (`fix_music_end`, `fix_music_restart`, `fix_music_mark`,
`fix_ireland_clicks`, each `on` or `off`); a `settings.ini` written by
an earlier version, with its single `minor_bug_fixes` line, is still
read: that line sets every fix. From the command line, `--hd` turns HD on and
`--widescreen` HD with widescreen, and `--original` turns every bug fix
and the arrow keys' turn off, for that run only; `--size 1920x1080` opens the window at that size,
for example `atlantis-native --widescreen --size 1920x1080
/path/to/Atlantis_2_Files`.

### The 800 x 600 screen

The original offers 640 x 480 and 800 x 600 on its options page (the
button beside the subtitles), and so does the port, as the original
does it: in the panorama the view is drawn with 800 x 600 pixels (the
same field of view, a finer picture; the bag bar shows 10 objects
instead of 8), while the films, the dialogs, the menus and the credits,
made for 640 x 480, are enlarged to 800 x 600 by the original's own
filter. Choose it with the options page's button, the port's options
menu (Screen) or `--resolution 800x600`; it is kept in `atlant2.cfg`. In
the HD mode the screen stays 640 x 480 (the HD picture is drawn from it):
the choice applies when HD is off.

## Command line

The game folder can be left out of each of these: the game library
decides (except with `--headless`).

`atlantis-native --launcher` opens on the game library's screen, even
with one game in it (holding Shift at start-up does the same). The Windows
release build has no console: redirect its output to a file
(`atlantis-native.exe --headless C:\game > out.txt`) to read it.

`atlantis-native --support-screen` shows the
[support screen](#the-support-screen) at start-up, whatever its counts
say.

`atlantis-native --new-game /path/to/game` skips the start-up videos and
the player screen.

`atlantis-native --original /path/to/game` plays the original game, bug for
bug (see [Enhancements](TECHNICAL.md#enhancements)); with Atlantis II too
(its music tracks then lose their last ~5.8 s when another follows, as
in the original).

`atlantis-native --hd /path/to/game` starts in the [HD mode](TECHNICAL.md#hd-mode),
whatever `settings.ini` says.

`atlantis-native --widescreen /path/to/game` starts in the HD mode with
the widescreen option on, for this run (`settings.ini` is not changed).

With Atlantis II, `--hd` and `--widescreen` turn on its HD mode for the
run, `--size 1920x1080` sizes its window and `--resolution 800x600`
chooses the original's 800 x 600 screen (see [Atlantis II](#atlantis-ii)).

`atlantis-native --fullscreen /path/to/game` opens the window in full
screen (F, F11 or Alt+Enter switches it back): for a launcher such as the
Ports of Batocera on a Raspberry Pi.

The environment variable `ATLANTIS_NATIVE_DATA` names another folder for
the per-user data (saves, settings, log): trial runs that must not touch
your own saves.

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

(`games.txt`, the game library, `support.ini`, the counts of the
[support screen](#the-support-screen), and `atlantis-native.log` are in
the `atlantis-native` folder itself.)

On its first start with a game whose folder holds an original
`players.atl` (an installation the original game played), the port copies
it and its checkpoints into that folder; the game folder is only read.
Files written by the port can be copied back for the original game.

Deleting a player deletes that player's own checkpoints; the players after
it move up with their files. (The original deletes the files of the letter
of the row, which after an earlier deletion can be another player's and
leaves the deleted player's files behind.)

The files are those the build you play writes: the 1997 English and
French CDs write a 95-byte `players.atl` (five chapter and letter bytes,
where the later builds write six), and the English one and the May 1997
French one store typed names in capitals.

The same folder holds `settings.ini`: the options of the game's menus
(subtitles, OMNI 3D mode, the master, music, effects and voices volumes,
which the original also keeps in each checkpoint and sets again when one
is loaded) and the [enhancements](TECHNICAL.md#enhancements), one `name = on|off` line
each: one per bug fix (`fix_music_loop`, `fix_head_overlay`,
`fix_head_first_frame`, `fix_bar_slide_out`, `fix_pickup_icon`,
`fix_menu_records`, `fix_fifth_player`, `fix_snakes_first_win`,
`fix_hexagon_pieces`, `fix_loadmusik`, `fix_sharp_s`,
`fix_disc_revision`, `fix_menu_line_width`, `fix_chinese_rows`,
`fix_subtitles_toggle`), `restored_content` and `ui_ux`. A
`settings.ini` written by an earlier version, with its single
`minor_bug_fixes` line, is still read: that line sets every fix. It also keeps `music_on_disk`, the music tracks the original game
would have copied to its folder (it reads them from the archives here),
so that this set outlives the session as the original's copies do,
`graphics = original|hd`, the graphics mode of the options menu (F10;
`original` by default), and the HD mode's picture options (see
[HD mode](TECHNICAL.md#hd-mode)): `hd_panorama = bicubic|bilinear`,
`hd_sharpen = on|off`, `hd_interface = smooth|pixels` and
`hd_sharp_subtitles = on|off`, `hd_widescreen = on|off` and
`hd_video_sides = black|blur|zoom|panorama` (by default `bicubic`, `on`,
`smooth`, `on`, `off`, `black`).

The web build keeps its players, checkpoints and settings in the
browser's storage (`localStorage`, one entry per file, kept per site and
per browser). It too takes over the `players.atl` and checkpoints of the
picked folder on its first start.

## Known differences from the original

- The window: the game runs in a resizable window (the picture scaled,
  in its 4:3 proportions) rather than fullscreen at 640x480.
- The saves are in a per-user folder, not in the game folder (see above);
  deleting a player deletes its own checkpoints.
- The subtitles of the 2001 and GOG.com builds are drawn with Liberation
  Sans, which has the metrics of the original's Arial (see
  [Subtitles](TECHNICAL.md#subtitles)), as are the August 1997 French
  and German CDs' and the Spanish and Italian CDs'; the 1997 English CD's, the May 1997 French CD's and
  the Dutch and Swedish CDs' are drawn with their own pixel font, as they
  did (the accents included). The Chinese CDs' are drawn with the
  player's own MingLiU or SimSun when `mingliu.ttc` (Traditional) or
  `simsun.ttc` (Simplified), from a Windows `Fonts` folder, is put in the
  game folder (or beside the disc images), else with the bundled Noto
  Serif TC or SC. A MingLiU older than version 7.00 (the one of Windows
  XP and earlier) is not used, although it is the player's own: its
  characters draw right only through the font's hinting programs, which
  the port does not run, so Noto Serif TC is drawn instead.
- The engine runs at a steady 60 steps per second; the original's loops
  run as fast as the machine allows.
- The [enhancements](TECHNICAL.md#enhancements) on by default: the music loops
  seamlessly; the SPF overlays of some talking heads are drawn over the
  head; the 3D mouth of some talking heads no longer jumps on the first
  frame of each loop of the video; on the German CD, the `ß` that the
  LOAD list's fonts lack is drawn as "ss" (the original leaves a gap);
  on the Dutch CD, the credits text and the player-name fonts are disc
  1's whatever the disc (the original shows disc 2's older revision while
  disc 2 is in use); on the Italian CD, the buttons and lines of the
  menus answer over the whole drawn word (the original's zones stop
  short of it: the right end of `OK` and `Annulla` does not answer); on
  the Simplified Chinese CD, every subtitle row starts and ends between
  two characters: the row after an ideographic space starts after the
  whole space (the original starts it in the middle of the space, and
  that row's characters come out wrong), and a punctuation mark is only
  taken where a character starts (the original also reads one across two
  characters and cuts in the middle of one): this mends the 72 shipped
  rows that come out wrong, in 50 lines; on the Chinese
  CDs, the options' SUBTITLES toggle answers over its own sprite while
  SUBTITLES is off (the original reads its record one byte out of step,
  so its click area takes a wrong width and height); each menu sets its
  click areas afresh at every visit (the original keeps one table of
  them for the whole session: on the May 1997 English and French
  CDs, after the sound options, the ARE YOU SURE ? prompt loses its left
  ornament, and on every CD the LOAD list's scrollbar knob, hidden while
  11 checkpoints or fewer were listed, stays hidden); with five players,
  OK on the fifth starts it with its name (the original empties that
  name, and the nameless player can no longer be chosen); and the
  others of [the options menu](#the-options-menu). Each can be turned
  off alone in the options menu's "Choose the bug fixes...";
  `--original` turns them all off.
- On the Traditional Chinese CD, the one subtitle with no punctuation
  mark in its first 75 bytes (a game-over narration) is cut at 75 bytes;
  what the original does with it is not known.
- On the Chinese CDs, the opening video's captions are drawn over that
  video only. When it is cut short, the original goes on drawing them
  from memory it has freed over the videos of the next moments (until 94
  seconds after the opening began); what that shows is not known.
- Three paths of the 3D talking heads that no shipped head reaches are
  not ported (see [Layout](TECHNICAL.md#layout)).
- F10 (or O, for a keyboard without function keys; not while a game menu
  takes letters) opens the port's [options menu](#the-options-menu); the [HD mode](TECHNICAL.md#hd-mode) it offers is
  off by default.

## The icon

The program file (Windows Explorer, shortcuts) and the macOS app carry
the project's own icon: a full moon over the sea with Atlantis' spiral
tower (`assets/icon/`, drawn by `make_icon.py`). While the game runs, the
window (title bar, taskbar, Alt+Tab) and the macOS Dock show the icon of
your own `atlantis.exe`, read at start-up like the executable's tables
(the game's pictures are not ours to distribute). The support screen
shows it at 84x84 (`atlantis-native-84.rgb565`, made by
`make_support_icon.py`).
