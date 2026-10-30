## Notes

[Atlantis Native](https://github.com/ripleyXLR8/atlantis-native) plays Cryo's
*Atlantis: The Lost Tales* (1997) from the original game's files: a
reimplementation of its engine in Rust, here with an SDL2 front end for
handhelds. The gamepad stands in for the mouse; an on-screen keyboard types
the players' names.

The SDL2 front end, its gamepad controls and its on-screen keyboard are
based on [Klaas Van den Broeck's (@Klaasvdb) R36S port](https://github.com/Klaasvdb/atlantis-native-handheld).

Thanks to Cryo Interactive for the game.

## Installation

Copy the files of your own copy of the game (a CD release or GOG.com) into
`ports/atlantisnative/gamedata/`: the folder holding `atlantis.exe`,
`BIGCD1.BIG` ... `BIGCD4.BIG` and its subfolders (`WAV`, `WAM`, `SCENAR`...).
Not needed: `SYSTEM`, `__support`, `unins000.*`, `Manual.pdf`.

Saves go to `ports/atlantisnative/saves/`, the last run's log to
`ports/atlantisnative/log.txt`.

## Controls

| Button               | Action                                        |
|----------------------|-----------------------------------------------|
| Left stick, D-pad    | Pointer (in the panoramas: look around)       |
| Right stick          | Pointer, slow (fine aim)                      |
| A                    | Left click                                    |
| B                    | Right click                                   |
| X                    | Space                                         |
| Y                    | Enter                                         |
| Start                | Escape (game menu, skip a video)              |
| L1 / R1 (held)       | Pointer slower / faster                       |
| Select (in a menu)   | On-screen keyboard on / off                   |
| Select (elsewhere)   | Options menu (enhancements, bug fixes)        |
| Select + Start       | Quit                                          |

On-screen keyboard: left / right choose a letter, A types it, B erases, X
types a space, Y switches upper / lower case, Start confirms (Enter).

Options menu: D-pad up / down chooses, left / right or A changes, B or
Select closes it.

A pad SDL does not know falls back to raw button numbers. An Xbox-style
pad (an Xbox Wireless Controller over Bluetooth, 8BitDo and the like, with
10 or 11 buttons) gets the Linux order (A B X Y LB RB View Start Xbox LS RS,
View and the Xbox button as Select, the right stick on axes 3 / 4, the D-pad
as a hat) and works without a `controls.ini`; any other pad gets the R36S's
numbering. To change it, run
`atlantis-native-sdl.aarch64 --input-test` over SSH in
`ports/atlantisnative`, press every button and write the numbers into
`ports/atlantisnative/controls.ini` (`a = 1`, `b = 0`, `start = 13`, `lx = 0`,
...). The same file takes `deadzone = 6000` (0..32767) and the speeds
`pointer_speed = 1.0` and `look_speed = 1.0`.
