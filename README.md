<div align="center">

# Atlantis Native

**Return to Atlantis, on today's machines.**

A native port of *Atlantis: The Lost Tales* (Cryo Interactive, 1997),
written in Rust, for Windows, macOS, Linux and your browser.

[![Build](https://github.com/ripleyXLR8/atlantis-native/actions/workflows/build.yml/badge.svg)](https://github.com/ripleyXLR8/atlantis-native/actions/workflows/build.yml)
[![Release](https://img.shields.io/github/v/release/ripleyXLR8/atlantis-native)](https://github.com/ripleyXLR8/atlantis-native/releases/latest)
[![Licence: GPL v3+](https://img.shields.io/badge/licence-GPLv3%2B-green)](LICENSE)
![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20macOS%20%7C%20Linux%20%7C%20Web-lightgrey)
![Rust](https://img.shields.io/badge/made%20with-Rust-orange)
[![Buy me a coffee](https://img.shields.io/badge/Buy%20me%20a%20coffee-ripleyxlr8-FFDD00?logo=buymeacoffee&logoColor=black)](https://buymeacoffee.com/ripleyxlr8)

[Download](#download) · [Quick start](#quick-start) · [Features](#features) · [Controls](#controls) · [Player's guide](docs/GUIDE.md) · [Technical notes](docs/TECHNICAL.md)

</div>

---

Seth, the queen's new guard, wakes up in Atlantis just as Queen Rhea
disappears. Atlantis Native brings back the whole adventure: the 360°
panoramas, the cinematics, the talking heads with their 3D mouths in time
with the voices, the seventeen puzzles and the end credits. It plays from
the first screen to the last.

It is the original game, **pixel for pixel and sample for sample**: the
engine is a translation of the Atlantis engine written for
[ScummVM](https://www.scummvm.org/)'s CryOmni3D engine, itself rebuilt by
reverse engineering the original executable. Each behaviour is traced to an
address in the original code. On top of that come optional fixes of the
original's bugs and an optional HD mode.

## Features

- 🖥️ **Windows, macOS (Apple Silicon), Linux and the web**: one engine, the
  same game everywhere, even in a browser tab.
- 🎯 **Faithful**: the scripts, dialogs, music, 3D heads and puzzles run the
  original's own logic. The heads are drawn with the original's arithmetic,
  so the pixels match on every platform.
- ✨ **HD mode** (optional): the panoramas and 3D heads rendered at your
  window's resolution, sharp subtitles, and the interface art upscaled with
  xBR. With **Widescreen** on, the panorama fills the whole of a wide
  window, the projection carried on past the original's edges; beside
  the videos, black bars, the frame's edges blurred, the frame zoomed to
  the width, or (for the moves) the panoramas of the places left and
  reached.
- 🐞 **Bug fixes** (optional): seamless music loops, talking-head overlays
  drawn correctly, and other fixes of the original's bugs. One switch turns
  them all off for the game exactly as it shipped.
- 💾 **Your saves, compatible**: the game saves at every chapter, in the
  original's own file format. An existing installation's players and
  checkpoints are taken over on the first start.
- ⚙️ **Options menu** (F10): graphics, HD settings and enhancements,
  changed at once while you play.
- 📚 **Game library**: several editions (GOG.com, CD, disc images...) in
  one list, to choose the one to play at start-up or switch from F10.
- 🪟 **A modern window**: resizable, the picture kept in its 4:3
  proportions, the mouse captured while you look around.

## Download

The [latest release](https://github.com/ripleyXLR8/atlantis-native/releases/latest)
holds the builds of the current version; the
[nightly release](https://github.com/ripleyXLR8/atlantis-native/releases/tag/nightly)
those of the latest commit:

| System | File |
|---|---|
| Windows 10 and later, 64-bit | `atlantis-native-windows-x64.zip` |
| macOS 11 and later, Apple Silicon | `atlantis-native-macos-arm64.dmg` |
| Linux x86-64 (glibc 2.35+, ALSA) | `atlantis-native-linux-x64.tar.gz` |
| Web (a static site to serve over HTTP; on the nightly release) | `atlantis-native-web.zip` |

> **Windows:** the program is not code-signed (a certificate costs money
> each year), so Windows SmartScreen may warn that it "protected your PC"
> the first time. Click **More info**, then **Run anyway**. It opens
> normally from then on. Download Atlantis Native only from its
> [releases page](https://github.com/ripleyXLR8/atlantis-native/releases):
> the builds there are made by this repository's public workflow.

> **macOS:** the app is signed ad hoc but not notarised by Apple (that
> needs a paid Apple Developer account), so macOS blocks it the first
> time. Since macOS 15 a right-click is not enough:
>
> 1. Open **Atlantis Native** once (macOS says it cannot verify it).
> 2. Go to **System Settings → Privacy & Security** and click
>    **Open Anyway** next to the message about Atlantis Native.
> 3. Confirm with **Open**. It opens normally from then on.
>
> Or, in the Terminal:
> `xattr -dr com.apple.quarantine "/Applications/Atlantis Native.app"`

## You need the game

Atlantis Native contains **no game data**. Bring your own copy:

| Release | Languages | What to point it at |
|---|---|---|
| CD (2001 build) | English | the folder with `atlantis.exe` and `BIGCD1.BIG` … `BIGCD4.BIG` |
| CD, retail release (May 1997 build) | English | the folder with `atlantis.exe` and `BIGCD1.BIG` … `BIGCD4.BIG` |
| CD, retail release (August 1997 build, 4 CDs) | French | the folder with `atlantis.exe` and `BIGCD1.BIG` … `BIGCD4.BIG` |
| CD, retail release, first pressing (May 1997 build, 4 CDs) | French | the folder with `atlantis.exe` and `BIGCD1.BIG` … `BIGCD4.BIG` |
| CD, retail release (August 1997 build, 4 CDs) | German | the folder with `atlantis.exe` and `BIGCD1.BIG` … `BIGCD4.BIG` |
| CD, retail release (June 1997 build, 4 CDs) | Dutch | the folder with `atlantis.exe` and `BIGCD1.BIG` … `BIGCD4.BIG` |
| CD, retail release (June 1997 build, 4 CDs) | Swedish | the folder with `atlantis.exe` and `BIGCD1.BIG` … `BIGCD4.BIG` |
| CD with Cryo's patch 1.01 (`AT1CDINT.EXE`, 1999 build) | English, French, German, Dutch or Swedish CD (French interface) | the folder with the patched `atlantis.exe` and `BIGCD1.BIG` … `BIGCD4.BIG` |
| [GOG.com](https://www.gog.com/) | English, French, German, Spanish, Italian | the installation folder |

Each build plays as it shipped: the 1997 English CD, the May 1997
French CD and the Dutch and Swedish CDs, for one, draw their subtitles in their own 8x8 pixel font (see [Game builds](docs/TECHNICAL.md#game-builds)).

How to get the files from your CD or GOG.com: see
[Getting the game files](docs/GUIDE.md#getting-the-game-files).

The game you chose is shown in the window's title and at the foot of the
options menu, e.g. *GOG.com, French (1999 build)*.

## Quick start

1. Download and unpack the build for your system.
2. Double-click **atlantis-native** (on macOS, **Atlantis Native**).
3. The first time, choose your game folder. It is remembered.
   With more than one edition, the
   [game library](docs/GUIDE.md#several-editions-the-game-library) lets
   you choose the one to play (hold Shift at start-up to open it).

That's it: the Cryo logo plays, create a player, and Atlantis awaits.

From a terminal you can also pass the folder:
`atlantis-native /path/to/game`. More options (`--original`, `--hd`,
`--new-game`, …) are in the [player's guide](docs/GUIDE.md#command-line).

## Controls

| Action | Control |
|---|---|
| Look around | move the mouse (or F5 / F6 / F7 / F8) |
| Go somewhere, talk, use | left click where the cursor animates |
| Next dialog line | left click |
| Skip a talking head's lines | Escape |
| Inventory | right click (the bar's edges scroll it) |
| In-game menu (load, options, quit) | Escape in the panoramic view |
| **Port options** (graphics, HD, bug fixes, game library) | **F10**, at any time; the game pauses |
| Diagnostic overlay | F9 |

On the player screen, click the last row to type a new name, then Enter
or OK.

## Build from source

```sh
cargo build --release
cargo run --release -- /path/to/game
```

On Linux, install the ALSA headers first (`libasound2-dev` or
`alsa-lib-devel`). The web build is made by `web/build.sh`; see the
[technical notes](docs/TECHNICAL.md#the-web-build).

## Learn more

- 📖 [Player's guide](docs/GUIDE.md): command line, where the saves and
  settings live, known differences from the original.
- 🔧 [Technical notes](docs/TECHNICAL.md): the engine's layout, the
  enhancements, how the HD mode works, subtitles, the web build, tests,
  roadmap.

## Code signing policy

Windows builds are to be signed with a free code signing certificate
provided by [SignPath.io](https://about.signpath.io/), certificate by
[SignPath Foundation](https://signpath.org/) (in progress: until then
the builds are unsigned).

- Committers and reviewers: [ripleyXLR8](https://github.com/ripleyXLR8)
- Approvers: [ripleyXLR8](https://github.com/ripleyXLR8)

Only the executable built by this repository's GitHub Actions workflow
from its own sources is signed; every release is approved by hand.

Privacy: this program does not transfer any information to other
networked systems. It only reads the game files you point it at and
writes its saves, settings and log to your own user folder.

## AI use

Atlantis Native was written for the most part by an AI (Anthropic's
Claude, through Claude Code), directed and reviewed by its maintainer:
the commits it wrote carry a `Co-Authored-By: Claude` line. The engine
follows the original game's code, reverse engineered and checked against
it: the tests compare the port's results with the original's own code run
under emulation (subtitles, talking heads, puzzles, tables), pixel for
pixel where it matters, on every edition supported. Reports of anything
that differs from the original are welcome.

## Support the project

Atlantis Native is free. If you enjoy it, you can
[buy me a coffee](https://buymeacoffee.com/ripleyxlr8) ☕: it helps keep
the work going (more editions, fixes, the HD and widescreen modes).
The link is also in the game: "Buy me a coffee..." in the F10 menu,
and at the foot of the game library.
Now and then (rarely, never more than once a week) a small thank-you
screen at start-up offers the same links; "Don't show this again" turns
it off (see the [guide](docs/GUIDE.md#the-support-screen)).

## Licence

GNU General Public License, version 3 or later (see `LICENSE`). The code is
derived from ScummVM's CryOmni3D engine, Copyright (C) the ScummVM team.

The program's icon (`assets/icon/`, drawn by `make_icon.py`) is the
project's own, under the same licence.

The subtitle font, Liberation Sans Regular
(`assets/fonts/LiberationSans-Regular.ttf`, Copyright (c) 2010 Google
Corporation, Copyright (c) 2012 Red Hat, Inc.), is licensed under the SIL
Open Font License, version 1.1 (see `assets/fonts/LICENSE-LiberationSans`).

*Atlantis: The Lost Tales* is a trademark of its owners. This project is not
affiliated with them.
