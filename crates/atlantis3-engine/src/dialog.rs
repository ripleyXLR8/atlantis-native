//! The dialogs (`CDialogue`): the lines of `com.txt` with their speakers
//! of `info.txt` and their texts of `Dial.txt`, the stands and cameras
//! picked at load and at each opening, and each line's body video, voice
//! and field of view.
//!
//! - Load (`AtlDial_MainInit` @ 0x0040b9c0, again at each `MONDE`): each
//!   `com.txt` line of a known speaker is an entry (`AddDialogEntry`
//!   @ 0x00464690); stand 0 is picked among the speaker's head files
//!   `mesh\<mesh>s<n>.db` (`rand() % count + 1`), or without one among the
//!   world's body videos `<mesh>s<n>c1.hnm`, the camera then too when it
//!   is 0 (`<mesh>s<stand>c<n>.hnm`).
//! - Opening (`CDialogue::Open` @ 0x00466140): each line whose speaker's
//!   head stand loads and whose camera is 0 (then always) gets a camera
//!   `rand() % cameras + 1` of the stand's `.db` (@ 0x004639c0).
//! - A line (`NextReplique` @ 0x00466890): the body video
//!   `hnmdial\HNM640\<world>\<mesh>s<stand>c<camera>.hnm`, the voice
//!   `loc\voix\<Langue>\<world>\<voice>.apc`, the field of view
//!   `[Focale<stand>][camera]` (76 by default).
//!
//! `rand()` is the C library's, shared with the rest of the game.

use std::collections::{BTreeMap, HashMap};

use atlantis2_engine::crt::Rand;
use atlantis3_data::dialog::{parse_com, parse_dial, parse_info, Perso};
use atlantis3_data::A3Files;

/// The players (@ 0x0074f350, `HERO` and `VOL` added at start-up
/// @ 0x004d684a): a speaker change from or to one turns the panorama.
pub const PLAYERS: [&str; 2] = ["hero", "vol"];

/// A line of a dialog (entry of 0x50 bytes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub speaker: String,
    pub stand: i32,
    pub camera: i32,
    pub mood: i32,
    /// The camera is picked again at every opening (+0x34).
    pub rnd: bool,
    pub voice: String,
    pub text: Vec<u8>,
}

/// One `rand()` drawn while loading or opening, with the count it was
/// reduced by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Draw {
    pub value: i32,
    pub n: i32,
}

/// The dialogs of a world.
pub struct Dialogs {
    pub persos: Vec<Perso>,
    /// The entries by dialog, then line.
    pub entries: BTreeMap<i32, BTreeMap<i32, Entry>>,
    /// The `rand()` values drawn by the load, per `com.txt` line in order
    /// (the lines of unknown speakers: none).
    pub load_draws: Vec<((i32, i32), Vec<Draw>)>,
    /// Cameras of each head stand (`<mesh>s<stand>.db`), `None` when the
    /// file does not exist.
    cameras: HashMap<(String, i32), Option<usize>>,
    /// The world's folder (`hoggar`, ...).
    pub world: String,
}

/// The placeholder text of an entry `Dial.txt` does not fill
/// (@ 0x0071c404).
pub const NO_TEXT: &[u8] = b"Pas de texte";

impl Dialogs {
    /// The load of world folder `world` (`hoggar`, `ldm`, ...): the
    /// stands and cameras picked with `rand`.
    pub fn load(files: &A3Files, world: &str, text_language: &str, rand: &mut Rand) -> Dialogs {
        let read = |p: &str| files.read(p.trim_start_matches("Datas/"));
        let persos = read(atlantis3_data::dialog::INFO_PATH)
            .and_then(|d| parse_info(&d).ok())
            .unwrap_or_default();
        let com = read(atlantis3_data::dialog::COM_PATH)
            .map(|d| parse_com(&d))
            .unwrap_or_default();
        let dial = read(&atlantis3_data::dialog::dial_path(text_language))
            .map(|d| parse_dial(&d).lines)
            .unwrap_or_default();
        let mut out = Dialogs {
            persos,
            entries: BTreeMap::new(),
            load_draws: Vec::new(),
            cameras: HashMap::new(),
            world: world.to_string(),
        };
        let exists = |p: String| files.size(&p).is_some();
        for c in com {
            let Some(mesh) = out.perso(&c.speaker).map(|p| p.mesh.clone()) else {
                // An unknown speaker: dropped without a message.
                continue;
            };
            let mut draws = Vec::new();
            let (mut stand, mut camera) = (c.stand, c.camera);
            if stand == 0 {
                let count =
                    |f: &dyn Fn(i32) -> String| (1..).take_while(|&n| exists(f(n))).count() as i32;
                let dbs = count(&|n| format!("mesh/{mesh}s{n}.db"));
                if dbs > 0 {
                    let v = rand.rand();
                    draws.push(Draw { value: v, n: dbs });
                    stand = v % dbs + 1;
                } else {
                    let vids = count(&|n| format!("hnmdial/HNM640/{world}/{mesh}s{n}c1.hnm"));
                    if vids > 0 {
                        let v = rand.rand();
                        draws.push(Draw { value: v, n: vids });
                        stand = v % vids + 1;
                        if camera == 0 {
                            let cams = count(&|n| {
                                format!("hnmdial/HNM640/{world}/{mesh}s{stand}c{n}.hnm")
                            });
                            if cams > 0 {
                                let v = rand.rand();
                                draws.push(Draw { value: v, n: cams });
                                camera = v % cams + 1;
                            }
                        }
                    }
                }
            }
            out.load_draws.push(((c.dialog, c.line), draws));
            out.entries.entry(c.dialog).or_default().insert(
                c.line,
                Entry {
                    speaker: c.speaker.clone(),
                    stand,
                    camera,
                    mood: c.mood,
                    rnd: false,
                    voice: c.voice.clone(),
                    text: NO_TEXT.to_vec(),
                },
            );
        }
        // `AddDialogText` (@ 0x00465200): an unknown speaker, dialog or
        // line is logged and ignored.
        for l in dial {
            if out.perso(&l.speaker).is_none() {
                continue;
            }
            if let Some(e) = out
                .entries
                .get_mut(&l.dialog)
                .and_then(|d| d.get_mut(&l.line))
            {
                e.text = l.text;
            }
        }
        // The heads' camera counts.
        let stands: Vec<(String, i32)> = out
            .entries
            .values()
            .flat_map(|d| d.values())
            .filter_map(|e| Some((out.perso(&e.speaker)?.mesh.clone(), e.stand)))
            .collect();
        for (mesh, stand) in stands {
            out.cameras.entry((mesh.clone(), stand)).or_insert_with(|| {
                if stand == 0 {
                    return None;
                }
                files
                    .read(&format!("mesh/{mesh}s{stand}.db"))
                    .and_then(|d| atlantis3_data::db::parse(&d).ok())
                    .map(|s| s.cameras.len())
            });
        }
        out
    }

    pub fn perso(&self, name: &str) -> Option<&Perso> {
        self.persos.iter().find(|p| p.name == name)
    }

    /// The number of lines of a dialog (`Open`: lines 1..count).
    pub fn count(&self, dialog: i32) -> i32 {
        self.entries.get(&dialog).map_or(0, |d| d.len() as i32)
    }

    pub fn entry(&self, dialog: i32, line: i32) -> Option<&Entry> {
        self.entries.get(&dialog)?.get(&line)
    }

    /// The cameras of a speaker's head stand; `None` when the head does
    /// not load (stand 0, no `.db`).
    pub fn camera_count(&self, speaker: &str, stand: i32) -> Option<usize> {
        let mesh = &self.perso(speaker)?.mesh;
        self.cameras.get(&(mesh.clone(), stand)).copied().flatten()
    }

    /// `CDialogue::Open(n)`'s camera picks (@ 0x004639c0), in line order;
    /// the draws made. Nothing for a voice-over dialog.
    pub fn open(&mut self, dialog: i32, voixoff: bool, rand: &mut Rand) -> Vec<Draw> {
        let mut draws = Vec::new();
        if voixoff {
            return draws;
        }
        for line in 1..=self.count(dialog) {
            let Some(e) = self.entry(dialog, line) else {
                continue;
            };
            let Some(n) = self.camera_count(&e.speaker, e.stand) else {
                continue;
            };
            let e = self
                .entries
                .get_mut(&dialog)
                .and_then(|d| d.get_mut(&line))
                .unwrap();
            if !e.rnd && e.camera == 0 {
                e.rnd = true;
            }
            if e.rnd {
                e.camera = if n != 0 {
                    let v = rand.rand();
                    draws.push(Draw {
                        value: v,
                        n: n as i32,
                    });
                    v % n as i32 + 1
                } else {
                    0
                };
            }
        }
        draws
    }

    /// The body video of a line (@ 0x0046a410), under `Datas\`.
    pub fn video_path(&self, e: &Entry) -> String {
        let mesh = self.perso(&e.speaker).map_or("", |p| p.mesh.as_str());
        format!(
            "hnmdial/HNM640/{}/{mesh}s{}c{}.hnm",
            self.world, e.stand, e.camera
        )
    }

    /// The voice of a line (@ 0x0046a760), under `Datas\`.
    pub fn voice_path(&self, e: &Entry, language: &str) -> String {
        format!("loc/voix/{language}/{}/{}.apc", self.world, e.voice)
    }

    /// The field of view of a line (`GetSpeakerFocale` @ 0x004685c0).
    pub fn focale(&self, e: &Entry) -> f32 {
        self.perso(&e.speaker)
            .map_or(atlantis3_data::dialog::DEFAULT_FOCALE, |p| {
                p.focale(e.stand, e.camera)
            })
    }

    /// Whether a speaker is a player.
    pub fn is_player(name: &str) -> bool {
        PLAYERS.iter().any(|p| p.eq_ignore_ascii_case(name))
    }
}
