//! The desktop program's command line: `--help`'s text and
//! `--data-dir <folder>` (the other options are read in `main.rs`).

use std::path::PathBuf;

/// `--help`'s text: every option, one line each, and where the data go.
pub const HELP: &str = "\
usage: atlantis-native [options] [<game folder>]

Without a game folder the game library opens (or its one game starts);
with one, that game is played (Atlantis, Atlantis II, Atlantis III or
Atlantis Evolution, told apart by the folder's files).

Options:
  -h, --help                 print this help and end
  --data-dir <folder>        keep the saves, settings, game library and log in <folder>
  --fullscreen               open in full screen for this run
  --original                 every enhancement and bug fix off: the original game
  --hd                       start in the HD mode (Atlantis, Atlantis II)
  --widescreen               start in the HD mode with the widescreen option (Atlantis, Atlantis II)
  --new-game                 skip the start-up videos and the player screen (Atlantis)
  --launcher                 show the game library even with one game (or hold Shift)
  --list-games               print the games found on this computer and end
  --discs <folder>           another folder to look for the CD images or copies in (Atlantis II, III)
  --no-intro                 skip the intro videos (Atlantis II, III; the 2004 logos)
  --size <w>x<h>             the window's size (Atlantis II)
  --resolution <640x480|800x600>  the original's screen size option (Atlantis II)
  --evolution2004 <folder> [--fullscreen]  play Atlantis Evolution's 2004 CD installation
  --atlantis5 <disc, image or folder> [--fullscreen] [--headless] [--screenshot <file.png>]
                             The Secrets of Atlantis (2006), in development: its first view

Data folder (saves, settings, game library, log), the first that applies:
  1. --data-dir <folder>
  2. the folder the ATLANTIS_NATIVE_DATA environment variable names
  3. portable mode: when a file portable.txt or a folder data is beside
     the executable (on macOS, beside the .app too), the folder data there
  4. the per-user folder: %APPDATA%\\atlantis-native (Windows),
     ~/Library/Application Support/atlantis-native (macOS),
     $XDG_DATA_HOME/atlantis-native or ~/.local/share/atlantis-native (Linux)

Development and testing:
  --headless <game folder> [frames]  run without a window and print what happened
  --screenshot <file.png>    with --headless: write the last screen (Atlantis II, III)
  --puzzle <n> [--object <id>]  play puzzle n at once, object id in hand (Atlantis)
  --support-screen           show the support screen at once (Atlantis)
  --intro                    play the intro in a headless run (Atlantis II)
  --view <id>[,<yaw>,<pitch>]  start in that view (Atlantis II)
  --dialog <n>               play dialog n at once (Atlantis II)
  --menu <page>              show a menu page: player, main, options, sound, load, credits (Atlantis II)
  --take <names>             put objects in the bag, comma separated (Atlantis II)
  --hold <name>              put an object in the hand (Atlantis II)
  --bag                      open the bag bar (Atlantis II)
  --pointer <x>,<y>          move the pointer (Atlantis II)
  --talk                     click the character under the centre (Atlantis II)
  --command <commands>       run script commands at the start (Atlantis II)
  --seed <n>                 the start-up random seed (Atlantis II)
  --options-menu             show the options menu in a headless screenshot (Atlantis II)
  --options-keys <keys>      press keys in it first: up, down, left, right, enter, escape (Atlantis II)
  --atlantis4-view <folder> [--node <node> | --save <file>] [--lang <de|en|es|fr|it>] [--cpu] [--gpu-check] [--fullscreen]
                             Atlantis Evolution's development viewer
";

/// `--data-dir <folder>` (or `--data-dir=<folder>`) taken out of `args`;
/// an error when the folder is missing.
pub fn take_data_dir(args: &mut Vec<String>) -> Result<Option<PathBuf>, String> {
    let mut found = None;
    while let Some(i) = args
        .iter()
        .position(|a| a == "--data-dir" || a.starts_with("--data-dir="))
    {
        let a = args.remove(i);
        let value = match a.strip_prefix("--data-dir=") {
            Some(v) => v.to_string(),
            None if i < args.len() => args.remove(i),
            None => return Err("--data-dir needs a folder".into()),
        };
        if value.is_empty() {
            return Err("--data-dir needs a folder".into());
        }
        found = Some(PathBuf::from(value));
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn data_dir_taken_out() {
        let mut a = args(&["--fullscreen", "--data-dir", "/d", "/game"]);
        assert_eq!(take_data_dir(&mut a), Ok(Some(PathBuf::from("/d"))));
        assert_eq!(a, args(&["--fullscreen", "/game"]));
        let mut a = args(&["--data-dir=/e", "/game"]);
        assert_eq!(take_data_dir(&mut a), Ok(Some(PathBuf::from("/e"))));
        assert_eq!(a, args(&["/game"]));
        let mut a = args(&["/game"]);
        assert_eq!(take_data_dir(&mut a), Ok(None));
        assert!(take_data_dir(&mut args(&["--data-dir"])).is_err());
        assert!(take_data_dir(&mut args(&["--data-dir="])).is_err());
    }

    #[test]
    fn help_names_every_flag_main_reads() {
        let main = include_str!("main.rs");
        let mut missing = Vec::new();
        for (i, _) in main.match_indices("\"--") {
            let rest = &main[i + 1..];
            let end = rest.find('"').unwrap();
            let flag = &rest[..end];
            if flag.len() > 2
                && flag[2..]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
                && !HELP.contains(flag)
            {
                missing.push(flag.to_string());
            }
        }
        assert!(missing.is_empty(), "not in --help: {missing:?}");
    }
}
