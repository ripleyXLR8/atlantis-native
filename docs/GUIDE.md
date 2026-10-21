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
bug (see [Enhancements](TECHNICAL.md#enhancements)).

`atlantis-native --hd /path/to/game` starts in the [HD mode](TECHNICAL.md#hd-mode),
whatever `settings.ini` says.

`atlantis-native --widescreen /path/to/game` starts in the HD mode with
the widescreen option on, for this run (`settings.ini` is not changed).

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
each. It also keeps `music_on_disk`, the music tracks the original game
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
  the Simplified Chinese CD, the subtitle row after an ideographic space
  starts after the whole space (the original starts it in the middle of
  the space, and that row's characters come out wrong): this mends 27 of
  the 72 shipped rows that come out wrong; the other 45 start in the
  middle of a character where the original reads a punctuation mark
  across two characters, and stay as in the original; on the Chinese
  CDs, the options' SUBTITLES toggle answers over its own sprite while
  SUBTITLES is off (the original reads its record one byte out of step,
  so its click area takes a wrong width and height).
  `--original` turns them off.
- On the Traditional Chinese CD, the one subtitle with no punctuation
  mark in its first 75 bytes (a game-over narration) is cut at 75 bytes;
  what the original does with it is not known.
- On the Chinese CDs, the opening video's captions are drawn over that
  video only. When it is cut short, the original goes on drawing them
  from memory it has freed over the videos of the next moments (until 94
  seconds after the opening began); what that shows is not known.
- On the GOG.com release, the 30 dialog lines marked `off1` are played
  with the view redrawn; the original shows the picture the last video
  (or music load) left under their subtitle (the port does so on the Chinese CDs only).
- Escape on the player screen quits.
- Three paths of the 3D talking heads that no shipped head reaches are
  not ported (see [Layout](TECHNICAL.md#layout)).
- F10 (or O, for a keyboard without function keys; not while a game menu
  takes letters) opens the port's options menu; the [HD mode](TECHNICAL.md#hd-mode) it offers is
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
