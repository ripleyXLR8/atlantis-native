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
explains what it needs and asks for the folder. It remembers the folder.

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
1997), the CD with Cryo's patch 1.01, and the GOG.com release in
English, French, German, Spanish and Italian. Another edition is
refused with the size of its `atlantis.exe`: open an issue on the
project's page with it and where your copy comes from, to help support
it.

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

## Command line

The game folder can be left out of each of these: the remembered one is
used, or the picker asks for it (except with `--headless`). The Windows
release build has no console: redirect its output to a file
(`atlantis-native.exe --headless C:\game > out.txt`) to read it.

`atlantis-native --new-game /path/to/game` skips the start-up videos and
the player screen.

`atlantis-native --original /path/to/game` plays the original game, bug for
bug (see [Enhancements](TECHNICAL.md#enhancements)).

`atlantis-native --hd /path/to/game` starts in the [HD mode](TECHNICAL.md#hd-mode),
whatever `settings.ini` says.

`atlantis-native --widescreen /path/to/game` starts in the HD mode with
the widescreen option on, for this run (`settings.ini` is not changed).

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
  CD's; the 1997 English CD's and the May 1997 French CD's are drawn with
  their own pixel font, as they did (the French accents included).
- The engine runs at a steady 60 steps per second; the original's loops
  run as fast as the machine allows.
- The [enhancements](TECHNICAL.md#enhancements) on by default: the music loops
  seamlessly; the SPF overlays of some talking heads are drawn over the
  head; the 3D mouth of some talking heads no longer jumps on the first
  frame of each loop of the video. `--original` turns them off.
- Escape on the player screen quits.
- Three paths of the 3D talking heads that no shipped head reaches are
  not ported (see [Layout](TECHNICAL.md#layout)).
- F10 opens the port's options menu; the [HD mode](TECHNICAL.md#hd-mode) it offers is
  off by default.

## The icon

The program file (Windows Explorer, shortcuts) and the macOS app carry
the project's own icon: a full moon over the sea with Atlantis' spiral
tower (`assets/icon/`, drawn by `make_icon.py`). While the game runs, the
window (title bar, taskbar, Alt+Tab) and the macOS Dock show the icon of
your own `atlantis.exe`, read at start-up like the executable's tables
(the game's pictures are not ours to distribute).
