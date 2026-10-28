//! Where the port may differ from the original game, by choice.
//!
//! The groups mirror the enhancement classes of ScummVM's CryOmni3D
//! Atlantis engine (`kEnhMinorBugFixes`, `kEnhRestoredContent`,
//! `kEnhUIUX`); the minor bug fixes are each their own switch here
//! ([`BugFix`], [`BugFixes`]). The original game, bug for bug, is every
//! switch off ([`Enhancements::original`]); the defaults are those of the
//! ScummVM port. Fixes of bugs that crash or block the original game are
//! not optional and are not listed here.

use atlantis_data::exe::{CjkBreaker, ExeBuild, LoadList};
use atlantis_data::Game;

/// A fix of a minor bug of the original, each one its own switch
/// ([`BugFixes`]; `settings.ini`'s [`BugFix::key`], the options menu's
/// "Choose the bug fixes..."). Off, that behaviour is the original's,
/// whatever the other fixes are. Some fix a bug of some builds only
/// ([`BugFix::applies`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BugFix {
    /// The music loops seamlessly: the track as decoded, looped. Off, it
    /// plays through the original's streaming ring (`FUN_00432ea8`,
    /// [`crate::music::MusicRing`]), whose stale fragments are heard where
    /// the sound driver's play position runs ahead of the refills. Read at
    /// each track start.
    MusicLoop,
    /// The dialog SPF overlay (the guards' coif, the pilot's scarf) is
    /// drawn over the talking head. Off, as the original, it is drawn into
    /// the locked surface, which the back-buffer copy then overwrites
    /// (`FUN_0042ee88` @ 0x0042f57e), so it is never seen.
    HeadOverlay,
    /// Frame 0 of a talking head's animation is drawn with frame 1's pose.
    /// Off, as the original, the first key of each track, a rest pose in
    /// some `*A.3DA` files (Meljanz, Actyon, Servage, the Queen, Gimbas):
    /// the 3D lower face is drawn up to 35 pixels away from the video's
    /// face on the first frame of each loop
    /// ([`crate::head::DialogHead::set_rest_key_fix`]).
    HeadFirstFrame,
    /// The inventory bar's slide-out draws no icon once the slots are at
    /// y 506. Off, as the original (`FUN_0042c59c` @ 0x0042c643 reads the
    /// y of a record from 9 on past the 9-entry slot table), a scrolled
    /// bar still shows 1..4 rows of those icons at 506 (with 16 records
    /// and the first shown at 9, the whole bar at 506 and 513;
    /// [`crate::ui::bar::slide_out_record_drawn`]).
    BarSlideOut,
    /// After a pick-up the new record's icon is drawn in its slot like the
    /// others. Off, as the original (`FUN_004278a0` @ 0x00427ae2), at
    /// x 70 c + 35, 7 px left, in the hold loop and the slide-out
    /// ([`crate::ui::bar::slot_x`]).
    PickupIcon,
    /// Each menu keeps its own click records, from zero at every visit.
    /// Off, the original's one record table for the session
    /// (`ui::layout::MenuRecordTable`): with the May 1997 English and
    /// French builds the sound menu's last two records spill over the ARE
    /// YOU SURE ? menu's and move its left ornament off screen for the
    /// session, and in every build the LOAD scrollbar's knob, hidden by a
    /// visit with 11 checkpoints or fewer, stays hidden on later visits.
    MenuRecords,
    /// With five players, OK on the fifth starts it with its name. Off, as
    /// the original (`FUN_0041dea4` @ 0x0041e6c6..0x0041e72f), OK runs the
    /// commit of a typed name, empty, over the fifth name: that player
    /// stays listed with no name, unselectable.
    FifthPlayer,
    /// The four snakes puzzle (StartPuzzle 8) is won only with every snake
    /// released. Off, as the original, on a first run the snakes not yet
    /// released hold the zeroed memory (place (0, 0), piece 0), so piece 0
    /// carried to the screen's (0, 0) wins
    /// ([`crate::puzzle::snakes::SnakesGame::frame`]).
    SnakesFirstWin,
    /// The hexagon puzzle always places its pieces, as the 2001 entry does
    /// (OBJETENMAIN set to 402 before its test). Off, as the 1997 and the
    /// Chinese builds (atlantis-cd-en-688128 FUN_0043c570), the pieces are
    /// placed only when the player enters holding object 402 or with
    /// FLAGTEMP6 = 2. Those builds only (`ExeBuild::hexa_forces_402`
    /// false).
    HexagonPieces,
    /// With the French May 1997 build, LOADMUSIK copies its tracks one by
    /// one, as the other builds do. Off, all or nothing on its first track
    /// (atlantis-cd-fr-688640 FUN_0041e830). Either way it copies nothing
    /// below a current music volume of 5 (FUN_00429184 @ 0x00429c5e). That
    /// build only (`ExeBuild::music_load_all_or_nothing`).
    LoadMusik,
    /// `ß` (0xe1, the German CD's EPI.TXT) in the sprite-font texts is
    /// drawn and measured as two 's'. Off, as the original: glyph 193,
    /// past the fonts' 137 sprites, is not drawn and the pen moves by the
    /// advance table's byte (7 in atlantis-cd-de-687104, FUN_00410430;
    /// [`crate::ui::layout::spell_sharp_s`]). The games whose texts hold
    /// `ß` ([`FixData::sharp_s`]): of the known data, the German CD's.
    SharpS,
    /// With the Dutch CD, CREDITS.TXT and the FONTMIN fonts are disc 1's
    /// newest revision whatever the disc. Off, the current disc's copy, as
    /// the original reads them from the disc in the drive: disc 2 holds an
    /// older revision (CREDITS.TXT one line shorter, FONTMIN's `j` and `'`
    /// hotspots). The games whose discs hold different copies of those
    /// files ([`FixData::disc_revisions`]): of the known data, the Dutch
    /// CD's.
    DiscRevision,
    /// With the Italian CD, a positioned menu line (`<x,y>`) answers over
    /// 22 px per byte as in every other build, so the whole drawn word
    /// highlights and takes the click. Off, 16 px per byte as the original
    /// (atlantis-cd-it-687104 FUN_0041ecb4 @ 0x0041f068): the right ends of
    /// `OK` (11 px) and `Annulla` (18 px) do not answer. That build only
    /// (`ExeBuild::menu_positioned_byte_width`).
    MenuLineWidth,
    /// With the Simplified Chinese CD, the subtitle rows start and end
    /// between characters: the row after an ideographic space starts after
    /// the whole space, and a punctuation mark is only taken where a
    /// character starts. Off, as the original (atlantis-cd-zh-cn-692736
    /// FUN_0042f424 @ 0x0042f5ef), the row after the space starts on its
    /// second byte and a pair across two characters can be taken for a
    /// mark, so those rows read their characters one byte out of step
    /// ([`crate::subtitles::cjk::CjkSplit::row_fix`]). That build only.
    ChineseRows,
    /// With the Chinese CDs, the options' SUBTITLES record is read in step
    /// while SUBTITLES is off. Off, as the original
    /// (atlantis-cd-zh-tw-696832 FUN_004201e8 @ 0x0042022b): its text
    /// pointer steps back one byte also when the id stays 9, so the
    /// record's x is 0 and each later field is read from inside the one
    /// before (a record placed by an earlier walk keeps its place, but its
    /// click area takes the shifted width and height). Those builds only
    /// (`ExeBuild::menu_toggle_sprites`).
    SubtitlesToggle,
    /// With the Dutch and Swedish DVDs, the subtitles show the letters their
    /// texts hold (é è ë ï ú, ä å ö Ä Å Ö), read in the DOS code page they
    /// are written in (the CDs' files, for those CDs' bitmap font). Off, as
    /// the original: its program draws them through Windows (GDI TextOutA,
    /// atlantis-dvd-sv-695808 FUN_0042f384) in the Windows code page, so
    /// `„ † ” Ž ™ ‚ ‰ Š ‹ £` stand in their place (and nothing for `Å`).
    /// Those builds only (`ExeBuild::dos_subtitle_texts`).
    DosLetters,
    /// With the Japanese DVD, its two publishers' logo films, which the
    /// disc lacks, are skipped at start-up without a word. Off, as the
    /// original (atlantis-dvd-ja-696832 WinMain @ 0x00419713..0x00419772,
    /// FUN_00423dcc @ 0x00423df2..0x00423e22): an error box "Fichier
    /// introuvable : \face_b\ubb_vue\logogaga.hnm", then another for
    /// `logospik.hnm`, each waiting for OK, before the Cryo logo. A film
    /// that is there plays either way. That build only
    /// (`ExeBuild::startup_logos`).
    StartupLogos,
}

impl BugFix {
    /// Every fix, in the menu's order: those of every build, then those of
    /// some builds.
    pub const ALL: [BugFix; 17] = [
        BugFix::MusicLoop,
        BugFix::HeadOverlay,
        BugFix::HeadFirstFrame,
        BugFix::BarSlideOut,
        BugFix::PickupIcon,
        BugFix::MenuRecords,
        BugFix::FifthPlayer,
        BugFix::SnakesFirstWin,
        BugFix::HexagonPieces,
        BugFix::LoadMusik,
        BugFix::SharpS,
        BugFix::DiscRevision,
        BugFix::MenuLineWidth,
        BugFix::ChineseRows,
        BugFix::SubtitlesToggle,
        BugFix::DosLetters,
        BugFix::StartupLogos,
    ];

    /// Its `settings.ini` key.
    pub fn key(self) -> &'static str {
        match self {
            BugFix::MusicLoop => "fix_music_loop",
            BugFix::HeadOverlay => "fix_head_overlay",
            BugFix::HeadFirstFrame => "fix_head_first_frame",
            BugFix::BarSlideOut => "fix_bar_slide_out",
            BugFix::PickupIcon => "fix_pickup_icon",
            BugFix::MenuRecords => "fix_menu_records",
            BugFix::FifthPlayer => "fix_fifth_player",
            BugFix::SnakesFirstWin => "fix_snakes_first_win",
            BugFix::HexagonPieces => "fix_hexagon_pieces",
            BugFix::LoadMusik => "fix_loadmusik",
            BugFix::SharpS => "fix_sharp_s",
            BugFix::DiscRevision => "fix_disc_revision",
            BugFix::MenuLineWidth => "fix_menu_line_width",
            BugFix::ChineseRows => "fix_chinese_rows",
            BugFix::SubtitlesToggle => "fix_subtitles_toggle",
            BugFix::DosLetters => "fix_dos_letters",
            BugFix::StartupLogos => "fix_startup_logos",
        }
    }

    fn bit(self) -> u32 {
        1 << BugFix::ALL.iter().position(|&f| f == self).unwrap_or(0)
    }

    /// Whether the fix changes anything in a game of build `build` whose
    /// data is `data` ([`FixData::of`]): the bug is that build's (or every
    /// build's), or its data's (the `ß`, the discs' revisions: the patch
    /// 1.01's data is the CD's it was installed over, whatever its
    /// executable's language).
    pub fn applies(self, build: &ExeBuild, data: &FixData) -> bool {
        match self {
            BugFix::HexagonPieces => !build.hexa_forces_402,
            BugFix::LoadMusik => build.music_load_all_or_nothing,
            BugFix::SharpS => data.sharp_s,
            BugFix::DiscRevision => data.disc_revisions,
            BugFix::MenuLineWidth => build.menu_positioned_byte_width != 22,
            BugFix::ChineseRows => build.cjk_breaker == CjkBreaker::Gb2312,
            BugFix::SubtitlesToggle => build.menu_toggle_sprites,
            BugFix::DosLetters => build.dos_subtitle_texts,
            BugFix::StartupLogos => !build.startup_logos.is_empty(),
            _ => true,
        }
    }

    /// Whether the fix is of every build's bug.
    pub fn of_every_build(self) -> bool {
        !matches!(
            self,
            BugFix::HexagonPieces
                | BugFix::LoadMusik
                | BugFix::SharpS
                | BugFix::DiscRevision
                | BugFix::MenuLineWidth
                | BugFix::ChineseRows
                | BugFix::SubtitlesToggle
                | BugFix::DosLetters
                | BugFix::StartupLogos
        )
    }

    /// The fixes that change anything in `game`, in the menu's order. It
    /// reads the members [`FixData::members`] names (the web build fetches
    /// them first).
    pub fn of_game(game: &Game) -> Vec<BugFix> {
        let data = FixData::of(game);
        BugFix::ALL
            .into_iter()
            .filter(|f| f.applies(game.exe.build, &data))
            .collect()
    }
}

/// What of a game's data decides whether some fixes change anything in it
/// ([`BugFix::applies`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FixData {
    /// A text the sprite fonts draw holds `ß` (0xe1; [`BugFix::SharpS`]):
    /// the credits, the menus ([`is_menu_text`]) but with the builds that
    /// draw their bytes above 0xa8 as spaces (`menu_high_bytes_as_space`),
    /// EPI.TXT's chapter names but with the builds whose LOAD list is
    /// sprites (`LoadList::Sprites`: the Chinese EPI.TXT's double-byte
    /// characters hold 0xe1). The typed player names cannot (0x20..0xa7).
    pub sharp_s: bool,
    /// A member read from the disc in the drive (FONTMIN.SPR,
    /// FONTMIN2.SPR, and CREDITS.TXT with the builds that read it at each
    /// run) has a copy on some disc that differs from the first disc's
    /// ([`BugFix::DiscRevision`]).
    pub disc_revisions: bool,
}

/// Whether `name` is a menu text the menu walk draws (`SPRLIST\*MENU.TXT`,
/// `ERRORn.TXT`, `WAIT.TXT`): not a sprite list, nor a leftover of the
/// developers that no build reads (the Japanese DVD's `ALLSAVE.TXT` and
/// `CREDTEST.TXT`, which hold `ß`).
fn is_menu_text(name: &str) -> bool {
    let Some(n) = name.strip_prefix("SPRLIST\\") else {
        return false;
    };
    n.ends_with("MENU.TXT") || (n.starts_with("ERROR") && n.ends_with(".TXT")) || n == "WAIT.TXT"
}

/// The members read from the disc in the drive; CREDITS.TXT only with the
/// builds that read it at each run (`ExeBuild::credits_read_per_run`).
const CURRENT_DISC_MEMBERS: [&str; 3] = [
    "SPRITE\\2D\\FONTMIN.SPR",
    "SPRITE\\2D\\FONTMIN2.SPR",
    "SPRLIST\\CREDITS.TXT",
];

impl FixData {
    /// The members [`FixData::of`] reads: each text of `SPRLIST` (in the
    /// archives, or unpacked beside them), then each disc's copy of the
    /// members read from the disc in the drive (the disc whose copy, for
    /// `BigArchive::set_disc_copy`).
    pub fn members(game: &Game) -> Vec<(String, Option<u8>)> {
        let big = &game.big;
        let is_text = |n: &str| n.to_ascii_uppercase().ends_with(".TXT");
        let mut texts: Vec<String> = big
            .names()
            .filter(|n| n.starts_with("SPRLIST\\") && is_text(n))
            .map(str::to_string)
            .collect();
        for n in big.source().list("SPRLIST") {
            let name = atlantis_data::big::key(&format!("SPRLIST\\{n}"));
            if is_text(&name) && !texts.contains(&name) && big.contains(&name) {
                texts.push(name);
            }
        }
        texts.sort();
        let mut out: Vec<(String, Option<u8>)> = texts.into_iter().map(|n| (n, None)).collect();
        let per_run = game.exe.build.credits_read_per_run;
        for name in CURRENT_DISC_MEMBERS {
            if name.ends_with("CREDITS.TXT") && !per_run {
                continue;
            }
            for disc in big.discs_of(name) {
                out.push((name.to_string(), Some(disc)));
            }
        }
        out
    }

    /// The game's data facts, from its [`FixData::members`].
    pub fn of(game: &Game) -> FixData {
        let big = &game.big;
        let read = |name: &str, disc: Option<u8>| {
            big.set_disc_copy(disc);
            let data = big.read(name);
            big.set_disc_copy(None);
            data
        };
        let build = game.exe.build;
        let epi_drawn = matches!(build.load_list, LoadList::EpiText);
        let menus_drawn = !build.menu_high_bytes_as_space;
        let mut data = FixData::default();
        let mut firsts: std::collections::HashMap<String, Vec<u8>> = Default::default();
        for (name, disc) in FixData::members(game) {
            let Some(bytes) = read(&name, disc) else {
                continue;
            };
            if bytes.contains(&crate::ui::layout::SHARP_S) {
                data.sharp_s |= match name.as_str() {
                    "SPRLIST\\CREDITS.TXT" => true,
                    "SPRLIST\\EPI.TXT" => epi_drawn,
                    n => is_menu_text(n) && menus_drawn,
                };
            }
            if disc.is_some() {
                match firsts.get(&name) {
                    None => {
                        firsts.insert(name, bytes);
                    }
                    Some(first) => data.disc_revisions |= *first != bytes,
                }
            }
        }
        data
    }
}

/// The bug fixes, each on or off (default all on; [`BugFixes::NONE`] is
/// the original, bug for bug).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct BugFixes {
    bits: u32,
}

impl BugFixes {
    /// Every fix on.
    pub const ALL: BugFixes = BugFixes::all(true);
    /// Every fix off: the original.
    pub const NONE: BugFixes = BugFixes::all(false);

    /// Every fix `on`.
    pub const fn all(on: bool) -> BugFixes {
        BugFixes {
            bits: if on { (1 << BugFix::ALL.len()) - 1 } else { 0 },
        }
    }

    /// Fix `f` alone.
    pub fn only(f: BugFix) -> BugFixes {
        let mut x = BugFixes::NONE;
        x.set(f, true);
        x
    }

    /// Every fix but `f`.
    pub fn all_but(f: BugFix) -> BugFixes {
        let mut x = BugFixes::ALL;
        x.set(f, false);
        x
    }

    pub fn get(&self, f: BugFix) -> bool {
        self.bits & f.bit() != 0
    }

    pub fn set(&mut self, f: BugFix, on: bool) {
        if on {
            self.bits |= f.bit();
        } else {
            self.bits &= !f.bit();
        }
    }

    /// How many of `fixes` are on.
    pub fn count_of(&self, fixes: &[BugFix]) -> usize {
        fixes.iter().filter(|&&f| self.get(f)).count()
    }

    /// How many fixes are on.
    pub fn count(&self) -> usize {
        self.count_of(&BugFix::ALL)
    }
}

impl Default for BugFixes {
    fn default() -> Self {
        BugFixes::ALL
    }
}

impl std::fmt::Debug for BugFixes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.count() {
            0 => write!(f, "BugFixes(none)"),
            n if n == BugFix::ALL.len() => write!(f, "BugFixes(all)"),
            _ => {
                let off: Vec<&str> = BugFix::ALL
                    .iter()
                    .filter(|&&x| !self.get(x))
                    .map(|x| x.key())
                    .collect();
                write!(f, "BugFixes(all but {})", off.join(", "))
            }
        }
    }
}

/// The optional differences from the original game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Enhancements {
    /// Fixes of minor bugs, each its own switch (default all on; see
    /// [`BugFix`]).
    pub fixes: BugFixes,
    /// Content the original never shows (default off):
    /// - each condition evaluation starts from a clear key buffer, so
    ///   CHAPI063's Anna timer lines can play (the original keeps the
    ///   buffer across a scan, so INVENTnnn reads a longer number);
    /// - with the 2001 build, a second death in the same section plays the
    ///   next narrator line, as the 1999 build does (`FUN_00426590`).
    pub restored_content: bool,
    /// Interface changes (default off):
    /// - the error box of a talking head whose INITDIAL row is missing
    ///   (`FUN_0042ee88` @ 0x0042f036) is not shown; the line is skipped
    ///   either way.
    pub ui_ux: bool,
}

impl Default for Enhancements {
    fn default() -> Self {
        Enhancements {
            fixes: BugFixes::ALL,
            restored_content: false,
            ui_ux: false,
        }
    }
}

impl Enhancements {
    /// The original game: every switch off.
    pub fn original() -> Self {
        Enhancements {
            fixes: BugFixes::NONE,
            restored_content: false,
            ui_ux: false,
        }
    }

    /// Whether fix `f` is on.
    pub fn fix(&self, f: BugFix) -> bool {
        self.fixes.get(f)
    }

    /// The defaults with only fix `f` of the bug fixes on.
    pub fn only_fix(f: BugFix) -> Self {
        Enhancements {
            fixes: BugFixes::only(f),
            ..Enhancements::default()
        }
    }

    /// The defaults with every fix of the bug fixes on but `f`.
    pub fn all_but_fix(f: BugFix) -> Self {
        Enhancements {
            fixes: BugFixes::all_but(f),
            ..Enhancements::default()
        }
    }

    /// The defaults with the bug fixes all `on` or all off.
    pub fn with_fixes(on: bool) -> Self {
        Enhancements {
            fixes: BugFixes::all(on),
            ..Enhancements::default()
        }
    }

    /// `self` (the stored choices, `settings.ini`'s) with what a change
    /// from `before` to `after` (the options menu's) changed, every other
    /// switch kept as stored: a run's `--original`, which holds for that
    /// run only, is not written with an unrelated change.
    pub fn with_changes(&self, before: &Enhancements, after: &Enhancements) -> Enhancements {
        fn pick<T: PartialEq + Copy>(stored: T, before: T, after: T) -> T {
            if before != after {
                after
            } else {
                stored
            }
        }
        let mut x = *self;
        for f in BugFix::ALL {
            let on = pick(self.fixes.get(f), before.fixes.get(f), after.fixes.get(f));
            x.fixes.set(f, on);
        }
        x.restored_content = pick(
            self.restored_content,
            before.restored_content,
            after.restored_content,
        );
        x.ui_ux = pick(self.ui_ux, before.ui_ux, after.ui_ux);
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlantis_data::exe::BUILDS;

    #[test]
    fn each_fix_its_own_switch() {
        assert_eq!(BugFixes::ALL.count(), BugFix::ALL.len());
        assert_eq!(BugFixes::NONE.count(), 0);
        let mut keys = std::collections::HashSet::new();
        for f in BugFix::ALL {
            assert!(keys.insert(f.key()) && f.key().starts_with("fix_"));
            let one = BugFixes::only(f);
            assert_eq!(one.count(), 1);
            assert!(one.get(f));
            for g in BugFix::ALL.into_iter().filter(|&g| g != f) {
                assert!(!one.get(g) && BugFixes::all_but(f).get(g));
            }
            assert!(!BugFixes::all_but(f).get(f));
        }
        assert_eq!(Enhancements::default().fixes, BugFixes::ALL);
        assert_eq!(Enhancements::original().fixes, BugFixes::NONE);
        assert_eq!(format!("{:?}", BugFixes::NONE), "BugFixes(none)");
        assert_eq!(
            format!("{:?}", BugFixes::all_but(BugFix::SharpS)),
            "BugFixes(all but fix_sharp_s)"
        );
    }

    /// Which builds each build-specific fix is listed for, and the fixes
    /// of the data (the German CD's `ß`, the Dutch CD's disc revisions)
    /// with whatever build.
    #[test]
    fn the_fixes_of_each_build() {
        let of = |name: &str, data: FixData| -> Vec<BugFix> {
            let b = BUILDS.iter().find(|b| b.name == name).unwrap();
            BugFix::ALL
                .into_iter()
                .filter(|f| !f.of_every_build() && f.applies(b, &data))
                .collect()
        };
        use BugFix::*;
        let plain = FixData::default();
        let german = FixData {
            sharp_s: true,
            ..plain
        };
        let dutch = FixData {
            disc_revisions: true,
            ..plain
        };
        for b in BUILDS.iter() {
            for f in BugFix::ALL.into_iter().filter(|f| f.of_every_build()) {
                assert!(f.applies(b, &plain), "{f:?} {}", b.name);
            }
        }
        assert_eq!(of("atlantis-cd-en-714240", plain), []);
        assert_eq!(of("atlantis-gog-714752", plain), []);
        assert_eq!(of("atlantis-gog-714752", german), [SharpS]);
        assert_eq!(of("atlantis-patch-fr-714240", plain), []);
        assert_eq!(of("atlantis-patch-fr-714240", german), [SharpS]);
        assert_eq!(of("atlantis-patch-fr-714240", dutch), [DiscRevision]);
        assert_eq!(of("atlantis-cd-en-688128", plain), [HexagonPieces]);
        assert_eq!(of("atlantis-cd-fr-687104", plain), [HexagonPieces]);
        assert_eq!(
            of("atlantis-cd-fr-688640", plain),
            [HexagonPieces, LoadMusik]
        );
        assert_eq!(of("atlantis-cd-de-687104", german), [HexagonPieces, SharpS]);
        assert_eq!(
            of("atlantis-cd-nl-688128", dutch),
            [HexagonPieces, DiscRevision]
        );
        assert_eq!(
            of("atlantis-cd-it-687104", plain),
            [HexagonPieces, MenuLineWidth]
        );
        assert_eq!(
            of("atlantis-cd-zh-tw-696832", plain),
            [HexagonPieces, SubtitlesToggle]
        );
        assert_eq!(
            of("atlantis-cd-zh-cn-692736", plain),
            [HexagonPieces, ChineseRows, SubtitlesToggle]
        );
        // The 1998 DVD's program (the hexagon not forced, as the 1997 and
        // the Chinese builds), its Italian menus and the Dutch and Swedish
        // DOS texts; the Perfect Edition's.
        for b in [
            "atlantis-dvd-en-695296",
            "atlantis-dvd-fr-695296",
            "atlantis-dvd-de-695296",
            "atlantis-dvd-es-695296",
            "atlantis-pe-2018-695296",
        ] {
            assert_eq!(of(b, plain), [HexagonPieces], "{b}");
        }
        assert_eq!(
            of("atlantis-dvd-de-695296", german),
            [HexagonPieces, SharpS]
        );
        assert_eq!(
            of("atlantis-dvd-it-695296", plain),
            [HexagonPieces, MenuLineWidth]
        );
        for b in ["atlantis-dvd-nl-695808", "atlantis-dvd-sv-695808"] {
            assert_eq!(of(b, plain), [HexagonPieces, DosLetters], "{b}");
        }
    }

    /// A run's `--original` (every switch off): a change of one fix in the
    /// menu writes that change only, the stored choices kept.
    #[test]
    fn a_change_keeps_the_stored_choices() {
        let mut stored = Enhancements::default();
        stored.fixes.set(BugFix::SnakesFirstWin, false);
        stored.restored_content = true;
        let run = Enhancements::original();
        let mut after = run;
        after.fixes.set(BugFix::MusicLoop, true);
        let s = stored.with_changes(&run, &after);
        assert_eq!(s, stored, "MusicLoop already stored on");
        let mut after = run;
        after.fixes.set(BugFix::HeadOverlay, false);
        assert_eq!(stored.with_changes(&run, &after), stored, "no change");
        let mut on = stored;
        on.fixes.set(BugFix::HeadOverlay, false);
        let mut run2 = run;
        run2.fixes.set(BugFix::HeadOverlay, true);
        assert_eq!(on.with_changes(&run2, &run), on);
        let mut want = stored;
        want.fixes.set(BugFix::HeadOverlay, false);
        assert_eq!(stored.with_changes(&run2, &run), want);
        // All off, all on: written, the rest kept.
        let all = Enhancements::with_fixes(true);
        let none = Enhancements {
            ui_ux: true,
            ..Enhancements::original()
        };
        let got = stored.with_changes(&all, &none);
        assert_eq!(got.fixes, BugFixes::NONE);
        assert!(got.restored_content && got.ui_ux);
        assert_eq!(stored.with_changes(&none, &all).fixes, BugFixes::ALL);
    }
}
