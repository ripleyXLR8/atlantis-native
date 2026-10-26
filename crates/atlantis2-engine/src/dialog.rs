//! The dialogs (`dlg_Dialog` @ 0x006cf638, the script key `dial`): a
//! dialog's lines, their voices, the speaker's body video with its 3D head
//! over the panorama, the lip-sync, the subtitles, the panorama turned for
//! the shots and the music ducked.
//!
//! - [`DialogScript`]: a world's `loc\Dial<XX>.txt`, `scenar\com<XX>.txt`
//!   and `scenar\info<XX>.txt` (`dlg_Dialog::LoadScript` @ 0x0045dd60);
//! - [`Dialog::initialise`]: the lines of one dialog, their speakers' heads
//!   and their cameras (`dlg_Dialog::Initialise` @ 0x0045ebc0,
//!   `AddReplique` @ 0x00460830);
//! - [`Dialog::tick`]: the 15 Hz tick of the dialog loop (@ 0x004801d0):
//!   the line sequencer (@ 0x00460ba0: video frame, head pose, end of the
//!   voice, next line) and the panorama of the line (field of view, the
//!   reverse shots);
//! - [`Dialog::draw`]: the dialog frame (@ 0x00461430): lip-sync, body
//!   video through its `AL` mask, head, subtitles.
//!
//! The engine ([`crate::Engine`]) drives them: `/set(dial=N)` is
//! [`crate::Engine::start_dialog`].
//!
//! The heads, the lip-sync and the rest of the dialog's float code run
//! under the x87 control word the game's code has when the dialog runs: no
//! code of the executable sets another one between the panorama's frames
//! and a dialog's (the only control-word loads are the assembly
//! rasterisers' 0x0e7f and the CRT's save-and-restore pairs), so on the
//! reference machine (CPU type 8) it is the 0x0e7f `warpPII` leaves, and
//! the II head rasteriser loads it again at each polygon. The dialog takes
//! the engine's word ([`Dialog::initialise`], [`TickContext`],
//! [`FrameContext`]) and hands back the one its heads leave
//! ([`Dialog::fpu`]). A head is drawn with the routines of the panorama's
//! CPU ([`Variant`]): `PII` the II rasteriser with the x87 assembly
//! transform, `C` the C ones.

use std::sync::Arc;

use atlantis2_data::db::Scene;
use atlantis2_data::dialog::{
    parse_com, parse_dial, parse_info_rounding, ComLine, DialLine, LineKind, Perso, World,
};
use atlantis2_data::tga::Tga;
use atlantis2_data::{A2Files, Folder};

use crate::camera::Camera;
use crate::crt::Rand;
use crate::fpu::{ftol, Fpu};
use crate::hd::{DialogHd, HdTexts, TextFont};
use crate::head::Head;
use crate::hnm_stream::HnmStream;
use crate::lipsync::LipSync;
use crate::sound::{volume_to_attenuation, Pcm, Voices, VOICE_DIALOG};
use crate::subtitle::SubtitleFont;
use crate::warp::Variant;

/// π as the float the reverse shot turns by (0x40490fdb).
pub const PI_F: f32 = f32::from_bits(0x4049_0fdb);
/// The pitch of a `DS` shot (@ 0x005df9dc, −0.356163 rad).
pub const DS_PITCH: f32 = f32::from_bits(0xbeb6_5aff);
/// π / 180 as the two floats the field of view is converted with (@
/// 0x005c678c, @ 0x005c62b8).
const DEG_PI: f32 = f32::from_bits(0x4049_0fdb);
const DEG_180: f32 = f32::from_bits(0x3bb6_0b61);
/// 1 / 32767 as a float (@ 0x005c63b8).
const RAND_SCALE: f32 = f32::from_bits(0x3800_0100);
/// The voices' rate (the analyser assumes it, @ 0x005c807c).
const VOICE_RATE: u32 = 22050;
/// The default field of view of a line, in degrees (@ 0x004654e0).
const DEFAULT_FOCALE: f32 = 76.0;

/// Why a dialog cannot start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogError {
    /// A script file is missing.
    NoFile(String),
    /// No line has this dialog number (the original then plays one line of
    /// a speaker `Dummy` with an error text: not reproduced).
    UnknownDialog(u32),
    /// `DIALOG SCRIPT ERROR` (the original stops there): the two files'
    /// ids differ, a line number is out of order, an unknown speaker or a
    /// bad parameter.
    Script(String),
}

impl std::fmt::Display for DialogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DialogError::NoFile(n) => write!(f, "{n} not found"),
            DialogError::UnknownDialog(n) => write!(f, "dialog {n} is in no line of the script"),
            DialogError::Script(s) => write!(f, "dialog script error: {s}"),
        }
    }
}

impl std::error::Error for DialogError {}

/// A world's dialog scripts.
#[derive(Clone, Debug)]
pub struct DialogScript {
    pub world: World,
    pub dial: Vec<DialLine>,
    pub com: Vec<ComLine>,
    pub persos: Vec<Perso>,
}

impl DialogScript {
    /// `dlg_Dialog::LoadScript` (@ 0x0045dd60) for `world`, its numbers
    /// stored as floats under `fpu`, the control word when the world is
    /// loaded (`monde`, @ 0x0047fe50).
    pub fn load(files: &A2Files, world: World, fpu: Fpu) -> Result<DialogScript, DialogError> {
        let [dial, com, info] = world.files();
        let read = |folder, name: &str| {
            files
                .read(folder, name)
                .ok_or_else(|| DialogError::NoFile(name.to_string()))
        };
        Ok(DialogScript {
            world,
            dial: parse_dial(&read(Folder::Loc, &dial)?),
            com: parse_com(&read(Folder::Scenar, &com)?),
            persos: parse_info_rounding(&read(Folder::Scenar, &info)?, fpu == Fpu::Chop),
        })
    }

    fn perso(&self, name: &str) -> Option<&Perso> {
        self.persos.iter().find(|p| p.name == name)
    }
}

/// A line of a dialog (a réplique, 0x4c bytes, @ 0x00464d30).
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// `NNN_MM`.
    pub id: String,
    /// Index of the speaker in the dialog's speakers.
    pub perso: usize,
    pub speaker: String,
    /// The stand, 0..7 (`standN` − 1); `None` for the kinds without a
    /// head (+4 = −1).
    pub stand: Option<usize>,
    /// The camera, 0-based (+8); `None` when there is no stand.
    pub camera: Option<usize>,
    /// `humeurN`, 1..5.
    pub mood: u32,
    pub kind: LineKind,
    /// The body video, relative to `datas_a2\hnm\`: `dial/<Mesh>s<stand +
    /// 1>c<camera + 1>.hnm`; `None` without stand or camera (`NIL`).
    pub video: Option<String>,
    /// The voice, in `datas_a2\apc\dial\`: `<NNN_MM>.apc`.
    pub voice: String,
    /// The subtitle (Windows-1252).
    pub text: Vec<u8>,
    /// The tick range of a `cinema` line's subtitle.
    pub from: i32,
    pub to: i32,
}

/// A speaker (`dlg_Perso`): its `info` block and its heads by stand.
struct Speaker {
    info: Perso,
    heads: Vec<Option<Head>>,
}

/// The voice of the current line (`AdpcmStream`, +0x41c).
struct VoiceLine {
    samples: Arc<Vec<i16>>,
    /// The scheduler time the voice started (@ 0x00661928).
    start: f64,
}

/// The kind of the current line, as the loop code reads it (@
/// 0x004616f0); −1 none.
fn kind_code(k: LineKind) -> i32 {
    match k {
        LineKind::Head => 0,
        LineKind::VoiceOver => 1,
        LineKind::VoiceOver0 => 2,
        LineKind::Cinema => 3,
    }
}

/// What the dialog needs of the game at a tick.
pub struct TickContext<'a> {
    pub files: &'a A2Files,
    pub camera: &'a mut Camera,
    pub voices: &'a Voices,
    /// The dialog voice volume, 0..127 (@ 0x005df974).
    pub voice_volume: i32,
    /// The master volume, 0..256.
    pub master: i32,
    /// The scheduler's clock, seconds.
    pub clock: f64,
    /// The screen's size (the field of view's projection).
    pub screen: (u32, u32),
}

/// What the dialog frame needs of the game.
pub struct FrameContext<'a> {
    /// The scheduler's clock when the frame is drawn, seconds.
    pub clock: f64,
    pub rand: &'a mut Rand,
    /// The subtitle font, `None` when it is missing.
    pub font: Option<&'a SubtitleFont>,
    /// The subtitles option (@ 0x005df970).
    pub subtitles: bool,
    /// The screen's size.
    pub screen: (usize, usize),
    /// The x87 control word the game's code has when the frame is drawn.
    pub fpu: Fpu,
    /// The HD mode (a port addition): the frame's pictures captured for
    /// the HD heads, and the texts recorded for the sharp subtitles;
    /// `None` without it.
    pub hd: Option<&'a mut DialogHd>,
    pub texts: Option<&'a mut HdTexts>,
}

/// A running dialog.
pub struct Dialog {
    pub number: u32,
    world: World,
    lines: Vec<Line>,
    speakers: Vec<Speaker>,
    /// The current line (+0x500), `None` before the first tick and after
    /// the last line.
    current: Option<usize>,
    /// The "next line" request (+0x504): set at the start, at the end of a
    /// voice, by a click.
    next: bool,
    /// The `cinema` tick counter (+0x508).
    cinema_ticks: i32,
    stream: Option<HnmStream>,
    voice: Option<VoiceLine>,
    lipsync: Option<LipSync>,
    /// The 7 weights of the last evaluation (+0x4dc).
    pub weights: [f32; 7],
    /// The previous speaker of the panorama code (@ 0x006cf2a4).
    prev_speaker: String,
    /// The hero spoke (@ 0x006cf2b0): the `voixoff0` turn needs it.
    hero_spoke: bool,
    /// The speaker's place animation hidden while it speaks (@
    /// 0x006c2e78): its `[IdText]` names.
    pub hidden: Option<Vec<String>>,
    /// The view's angles when the dialog started (@ 0x006cf2c0, @
    /// 0x006cfb50), radians.
    pub saved_yaw: f32,
    pub saved_pitch: f32,
    /// A click to take into account at the next tick (@ 0x006569c9).
    click: bool,
    /// The x87 control word as the dialog's code last left it.
    fpu: Fpu,
    /// The CPU's routines the heads are drawn with.
    variant: Variant,
}

impl Dialog {
    /// `dlg_Dialog::Initialise(number)` (@ 0x0045ebc0): the lines of
    /// dialog `number`, each speaker's head for each stand used (`<Mesh>s<
    /// stand + 1>.db` with its maps), each line's camera (given, kept from
    /// the previous line of the same speaker, else drawn with `rand`).
    pub fn initialise(
        number: u32,
        script: &DialogScript,
        files: &A2Files,
        rand: &mut Rand,
        variant: Variant,
        fpu: Fpu,
    ) -> Result<Dialog, DialogError> {
        let err = |s: String| DialogError::Script(format!("dial {number}: {s}"));
        let mut lines: Vec<Line> = Vec::new();
        let mut speakers: Vec<Speaker> = Vec::new();
        // AddReplique's memory of the previous line (@ 0x006618f8, @
        // 0x00661944, @ 0x006617e4), cleared here.
        let mut prev_name = String::new();
        let (mut prev_stand, mut prev_cam) = (-1i32, -1i32);
        let mut found = false;
        for (k, d) in script.dial.iter().enumerate() {
            let Some(c) = script.com.get(k) else { break };
            let (Some((dn, mm)), cid) = (d.numbers(), &c.id) else {
                continue;
            };
            if &d.id != cid {
                return Err(err(format!("{} and {cid} differ", d.id)));
            }
            if dn != number {
                if found {
                    break;
                }
                continue;
            }
            found = true;
            if mm as usize != lines.len() + 1 {
                return Err(err(format!("line {} out of order", d.id)));
            }
            if c.speaker.is_empty() {
                return Err(err(format!("{}: no speaker", d.id)));
            }
            let info = script
                .perso(&c.speaker)
                .ok_or_else(|| err(format!("{}: unknown speaker {}", d.id, c.speaker)))?;
            if !(1..=5).contains(&c.mood) {
                return Err(err(format!("{}: humeur{}", d.id, c.mood)));
            }
            if !(1..=8).contains(&c.stand) {
                return Err(err(format!("{}: stand{}", d.id, c.stand)));
            }
            if c.camera == Some(0) {
                return Err(err(format!("{}: camera0", d.id)));
            }
            let stand = (c.kind == LineKind::Head).then_some(c.stand as usize - 1);
            // FUN_004605e0: the speaker, and its head for this stand.
            let p = match speakers.iter().position(|s| s.info.name == c.speaker) {
                Some(p) => p,
                None => {
                    speakers.push(Speaker {
                        info: info.clone(),
                        heads: (0..8).map(|_| None).collect(),
                    });
                    speakers.len() - 1
                }
            };
            if let Some(s) = stand {
                if speakers[p].heads[s].is_none() {
                    speakers[p].heads[s] = load_head(files, &info.mesh, s, variant, fpu);
                }
            }
            // AddReplique: the camera.
            let given = c.camera.map_or(-1, |n| n as i32 - 1);
            let mut cam = given;
            if c.speaker == prev_name && prev_stand != -1 && prev_cam != -1 && given == -1 {
                cam = prev_cam;
            }
            let stand_i = stand.map_or(-1, |s| s as i32);
            if cam == -1 && stand_i != -1 {
                let n = stand
                    .and_then(|s| speakers[p].heads[s].as_ref())
                    .map_or(0, |h| h.camera_count() as i32);
                cam = if n < 1 {
                    0
                } else {
                    // ftol(rand() × (1/32767) × (n − 0.1)), at most n − 1.
                    let r = f64::from(rand.rand());
                    let x = fpu.mul(
                        fpu.mul(r, f64::from(RAND_SCALE)),
                        fpu.sub(f64::from(n), 0.1),
                    );
                    ftol(x).min(n - 1)
                };
            }
            prev_name = c.speaker.clone();
            prev_stand = stand_i;
            prev_cam = cam;
            let video = match (stand, cam) {
                (Some(s), c) if c >= 0 => {
                    Some(format!("dial/{}s{}c{}.hnm", info.mesh, s + 1, c + 1))
                }
                _ => None,
            };
            lines.push(Line {
                id: d.id.clone(),
                perso: p,
                speaker: c.speaker.clone(),
                stand,
                camera: (cam >= 0).then_some(cam as usize),
                mood: c.mood,
                kind: c.kind,
                video,
                voice: format!("{}.apc", d.id),
                text: d.text.clone(),
                from: c.from as i32,
                to: c.to as i32,
            });
        }
        if lines.is_empty() {
            return Err(DialogError::UnknownDialog(number));
        }
        log::info!(
            "DIALOG: Initialisation (dial:{number}), {} lines",
            lines.len()
        );
        Ok(Dialog {
            number,
            world: script.world,
            lines,
            speakers,
            current: None,
            next: true,
            cinema_ticks: -1,
            stream: None,
            voice: None,
            lipsync: None,
            weights: [0.0; 7],
            prev_speaker: String::new(),
            hero_spoke: false,
            hidden: None,
            saved_yaw: 0.0,
            saved_pitch: 0.0,
            click: false,
            fpu,
            variant,
        })
    }

    /// The x87 control word the dialog's code left (0x0e7f once a II head
    /// has been drawn).
    pub fn fpu(&self) -> Fpu {
        self.fpu
    }

    /// The routines the heads are drawn with.
    pub fn variant(&self) -> Variant {
        self.variant
    }

    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    /// The dialog's number.
    pub fn number(&self) -> u32 {
        self.number
    }

    /// The current line's index.
    pub fn current(&self) -> Option<usize> {
        self.current
    }

    pub fn current_line(&self) -> Option<&Line> {
        self.lines.get(self.current?)
    }

    /// The kind of the current line, −1 when none (@ 0x004616f0).
    pub fn kind(&self) -> i32 {
        self.current_line().map_or(-1, |l| kind_code(l.kind))
    }

    /// The speakers' names and, per stand, the cameras of their heads.
    pub fn speakers(&self) -> Vec<(String, Vec<(usize, usize)>)> {
        self.speakers
            .iter()
            .map(|s| {
                let heads = s
                    .heads
                    .iter()
                    .enumerate()
                    .filter_map(|(i, h)| h.as_ref().map(|h| (i, h.camera_count())))
                    .collect();
                (s.info.name.clone(), heads)
            })
            .collect()
    }

    /// The body video of the current line.
    pub fn stream(&self) -> Option<&HnmStream> {
        self.stream.as_ref()
    }

    /// The head of the current line.
    pub fn head_mut(&mut self) -> Option<&mut Head> {
        let l = self.lines.get(self.current?)?;
        let (p, s) = (l.perso, l.stand?);
        self.speakers[p].heads[s].as_mut()
    }

    /// The lip-sync analysis of the current line's voice.
    pub fn lipsync(&self) -> Option<&LipSync> {
        self.lipsync.as_ref()
    }

    /// A click (or key) during the dialog: the next tick goes to the next
    /// line (@ 0x004801d0's first test).
    pub fn click(&mut self) {
        self.click = true;
    }

    /// The voice's samples and start time.
    pub fn voice(&self) -> Option<(&[i16], f64)> {
        self.voice.as_ref().map(|v| (&v.samples[..], v.start))
    }

    /// Whether the voice still plays at `clock`: DirectSound's status of
    /// voice 1 (FUN_0047a9c0(1) @ 0x0046101b), so a line ends once the
    /// mixer has played its voice out. When no sound output plays the mix
    /// (the tests and headless runs; a window without a sound device plays
    /// the mix into nothing), the voice lasts its
    /// samples from its start on the clock; with one, the port's guard: a
    /// voice the output has not played out a second after its length
    /// (a stalled device) ends by the clock too.
    fn voice_playing(&self, clock: f64, voices: &Voices) -> bool {
        let Some(v) = self.voice.as_ref() else {
            return false;
        };
        let end = v.start + v.samples.len() as f64 / f64::from(VOICE_RATE);
        if voices.output_attached() {
            voices.is_playing(VOICE_DIALOG) && clock < end + 1.0
        } else {
            clock < end
        }
    }

    /// The line sequencer (@ 0x00460ba0). `false` past the last line.
    fn sequencer(&mut self, ctx: &mut TickContext) -> bool {
        self.fpu = ctx.camera.fpu;
        if self.kind() == 3 {
            self.cinema_ticks += 1;
            if self
                .current_line()
                .is_some_and(|l| l.to < self.cinema_ticks)
            {
                self.next = true;
            }
        }
        if !self.next {
            let Some(line) = self.current_line() else {
                return true;
            };
            let stand = line.stand;
            let voiced = self.voice.is_some();
            if voiced && !self.voice_playing(ctx.clock, ctx.voices) {
                self.next = true;
            }
            if let Some(s) = stand {
                if let Some(st) = self.stream.as_mut() {
                    if let Err(e) = st.next_frame() {
                        log::warn!("{}: {e}", st.path());
                    }
                }
                let n = self.stream.as_ref().map_or(0, HnmStream::number);
                let p = self.lines[self.current.unwrap()].perso;
                if let Some(h) = self.speakers[p].heads[s].as_mut() {
                    h.set_fpu(self.fpu);
                    h.pose(n);
                }
            }
            return true;
        }
        // Next line.
        self.next = false;
        self.lipsync = None;
        ctx.voices.stop(VOICE_DIALOG);
        self.voice = None;
        let i = self.current.map_or(0, |c| c + 1);
        if i >= self.lines.len() {
            self.current = None;
            return false;
        }
        self.current = Some(i);
        let line = self.lines[i].clone();
        if let Some(s) = line.stand {
            let same = self
                .stream
                .as_ref()
                .is_some_and(|st| Some(st.path()) == line.video.as_deref());
            if same {
                if let Some(st) = self.stream.as_mut() {
                    if let Err(e) = st.next_frame() {
                        log::warn!("{}: {e}", st.path());
                    }
                }
            } else {
                self.stream = None;
                if let Some(v) = &line.video {
                    match ctx.files.read(Folder::Hnm, v) {
                        Some(data) => match HnmStream::open(v, Arc::from(data)) {
                            Ok(st) => self.stream = Some(st),
                            Err(e) => log::warn!("void HnmStream::Open(string) ERROR: {v}: {e}"),
                        },
                        None => log::warn!("void HnmStream::Open(string) ERROR: {v} not found"),
                    }
                }
            }
            let n = self.stream.as_ref().map_or(0, HnmStream::number);
            if let Some(h) = self.speakers[line.perso].heads[s].as_mut() {
                h.set_fpu(self.fpu);
                h.reset_idle();
                h.pose(n);
            }
        }
        // The voice: decoded whole, analysed, played on voice 1.
        let apc = ctx.files.read(Folder::ApcDial, &line.voice);
        let decoded = apc.as_deref().map(atlantis_codecs::decode_apc);
        match decoded {
            Some(Ok(v)) => {
                let samples = Arc::new(v.samples);
                let info = &self.speakers[line.perso].info;
                let matrix = info.matrix.unwrap_or([0.0; 70]);
                let n = samples.len() as u32;
                self.lipsync = Some(LipSync::new(self.fpu, &matrix, &samples, n, info.smooth));
                ctx.voices.play(
                    VOICE_DIALOG,
                    Pcm {
                        rate: v.sample_rate,
                        channels: v.channels,
                        samples: samples.clone(),
                    },
                    false,
                    volume_to_attenuation(
                        &crate::sound::attenuation_table(),
                        ctx.master,
                        ctx.voice_volume,
                    ),
                    0,
                );
                self.voice = Some(VoiceLine {
                    samples,
                    start: ctx.clock,
                });
            }
            Some(Err(e)) => log::warn!("{}: {e}", line.voice),
            None => log::info!("{}: no voice", line.voice),
        }
        true
    }

    /// The field of view of the current line, degrees (@ 0x00461970 →
    /// @ 0x004654e0): `[Focale<stand + 1>]` of the speaker, the value of
    /// the line's camera; 76 when there is none (stands 5..8 have none).
    pub fn focale(&self) -> f32 {
        let Some(l) = self.current_line() else {
            return DEFAULT_FOCALE;
        };
        let (Some(s), Some(c)) = (l.stand, l.camera) else {
            return DEFAULT_FOCALE;
        };
        self.speakers[l.perso]
            .info
            .focales
            .get(s)
            .and_then(|v| v.get(c))
            .copied()
            .unwrap_or(DEFAULT_FOCALE)
    }

    /// The tick of the dialog loop (@ 0x004801d0). `false` when the dialog
    /// is over (the engine then ends it, @ 0x004808a0).
    pub fn tick(&mut self, ctx: &mut TickContext) -> bool {
        if self.click {
            self.next = true;
            self.click = false;
        }
        if !self.sequencer(ctx) {
            if self.prev_speaker == "DS" {
                ctx.camera.pitch(DS_PITCH);
            }
            return false;
        }
        let hero = self.world.hero();
        let speaker = self
            .current_line()
            .map(|l| l.speaker.clone())
            .unwrap_or_default();
        match self.kind() {
            1 => return true,
            2 => {
                if self.hero_spoke && speaker != self.prev_speaker && speaker != hero {
                    if self.prev_speaker == hero {
                        ctx.camera.yaw(PI_F);
                    }
                    self.prev_speaker = speaker;
                }
                return true;
            }
            _ => {}
        }
        let fpu = ctx.camera.fpu;
        if self.prev_speaker.is_empty() {
            // The first line levels the view.
            ctx.camera.pitch(self.saved_pitch);
        }
        let fov = fpu.f32(fpu.mul(
            fpu.mul(f64::from(self.focale()), f64::from(DEG_PI)),
            f64::from(DEG_180),
        ));
        ctx.camera.set_fov(fov, ctx.screen.0, ctx.screen.1);
        if speaker == self.prev_speaker {
            return true;
        }
        self.hidden = None;
        // A `DS` shot looks 0.356 rad lower, and back up after.
        if speaker == "DS" {
            ctx.camera.pitch(-DS_PITCH);
        } else if self.prev_speaker == "DS" {
            ctx.camera.pitch(DS_PITCH);
        }
        if speaker != hero {
            if self.prev_speaker == hero {
                ctx.camera.yaw(PI_F);
            }
            self.prev_speaker = speaker;
            // FUN_00461870: the speaker's place animation is hidden.
            let l = self.current_line().unwrap();
            let ids = self.speakers[l.perso].info.id_texts.clone();
            self.hidden = (!ids.is_empty()).then_some(ids);
        } else {
            self.hero_spoke = true;
            self.prev_speaker = speaker;
            ctx.camera.yaw(PI_F);
        }
        true
    }

    /// The end of a dialog (@ 0x004808a0): stopped during a hero's line,
    /// the view turns back and the field of view returns to 76°; the
    /// hidden animation shows again; the voice stops (`Remove` @
    /// 0x00460290).
    pub fn end(&mut self, camera: &mut Camera, voices: &Voices, screen: (u32, u32)) {
        let hero = self.world.hero();
        if self.current_line().is_some_and(|l| l.speaker == hero) {
            camera.yaw(PI_F);
            camera.set_fov(crate::camera::WARP_FOV, screen.0, screen.1);
        }
        self.remove(voices);
    }

    /// The step of a dialog played over a film (@ 0x00480830): the line
    /// sequencer alone, its result ignored (the film's end ends the
    /// dialog). No click, no camera.
    pub fn film_step(&mut self, ctx: &mut TickContext) {
        self.sequencer(ctx);
    }

    /// `Remove` (@ 0x00460290): the voice stops and the dialog is emptied;
    /// all a dialog over a film gets at the film's end (@ 0x00480a40).
    pub fn remove(&mut self, voices: &Voices) {
        self.hidden = None;
        voices.stop(VOICE_DIALOG);
        self.current = None;
        self.stream = None;
        self.voice = None;
        self.lipsync = None;
    }

    /// The lip-sync time of the frame drawn at `clock` (@ 0x004610ee ..
    /// 0x00461134): (float)(clock − start) + `[Offset]`, as a float.
    pub fn lip_time(&self, clock: f64) -> Option<f32> {
        let v = self.voice.as_ref()?;
        let l = self.current_line()?;
        let off = self.speakers[l.perso].info.offset;
        let fpu = self.fpu;
        let dt = fpu.f(fpu.sub(clock, v.start));
        Some(fpu.f32(fpu.add(dt, f64::from(off))))
    }

    /// The mouth of the frame drawn at `clock` (@ 0x00461090 →
    /// `dlg_Anim::LipSync` @ 0x004631a0): the weights of the voice at the
    /// lip-sync time and the morph mix, or the idle mouth without a voice.
    pub fn update_mouth(&mut self, clock: f64, rand: &mut Rand) {
        let Some(l) = self.current_line() else { return };
        let (Some(s), p, mood) = (l.stand, l.perso, l.mood) else {
            return;
        };
        let t = self.lip_time(clock);
        let fpu = self.fpu;
        let weights = match (self.lipsync.as_mut(), t) {
            (Some(ls), Some(t)) => {
                ls.set_fpu(fpu);
                Some(ls.eval(t))
            }
            _ => None,
        };
        let Some(h) = self.speakers[p].heads[s].as_mut() else {
            return;
        };
        h.set_fpu(fpu);
        match weights {
            Some(w) => {
                self.weights = w;
                h.mouth(mood, &w);
            }
            None => h.idle_mouth(mood, rand),
        }
    }

    /// The dialog frame (@ 0x00461430) over the picture already in `out`
    /// (the panorama for a head line): the mouth, the body video, the head,
    /// then the subtitles when `subtitles` is on.
    pub fn draw(&mut self, out: &mut [u16], zbuf: &mut [u32], frame: &mut FrameContext) {
        let FrameContext {
            clock,
            ref mut rand,
            font,
            subtitles,
            screen,
            fpu,
            ref mut hd,
            ref mut texts,
        } = *frame;
        self.fpu = fpu;
        let kind = self.kind();
        if kind < 0 {
            return;
        }
        if kind == 0 {
            self.update_mouth(clock, rand);
            let l = self.current_line().unwrap();
            let cam = l.camera.unwrap_or(0);
            let (p, s) = (l.perso, l.stand);
            let mut head = s.and_then(|s| self.speakers[p].heads[s].as_mut());
            if let Some(h) = head.as_mut() {
                h.set_fpu(self.fpu);
            }
            if let Some(f) = compose_with(
                out,
                zbuf,
                screen.0,
                self.stream.as_ref(),
                head.map(|h| (h, cam)),
                rand,
                hd.as_deref_mut(),
            ) {
                self.fpu = f;
            }
        }
        if subtitles {
            if kind == 3 {
                let l = self.current_line().unwrap();
                if self.cinema_ticks <= l.from || l.to <= self.cinema_ticks {
                    return;
                }
            }
            if let Some(font) = font {
                let calls = font.layout(
                    &self.current_line().unwrap().text,
                    screen.0 as i32,
                    screen.1 as i32,
                );
                match texts.as_deref_mut() {
                    Some(t) => t.draw(TextFont::Dialog, font, out, screen.0, screen.1, &calls),
                    None => font.draw(out, screen.0, screen.1, &calls),
                }
            }
        }
    }
}

/// The body video and the head over the picture in `out` (rows of `pitch`
/// pixels), as the dialog frame does it (@ 0x00461446..0x0046165c): the Z
/// buffer cleared, the video drawn through its mask (@ 0x0046b790), then
/// the head with the line's camera (`DisplayFace` @ 0x00463460).
pub fn compose(
    out: &mut [u16],
    zbuf: &mut [u32],
    pitch: usize,
    stream: Option<&HnmStream>,
    head: Option<(&mut Head, usize)>,
    rand: &mut Rand,
) -> Option<Fpu> {
    compose_with(out, zbuf, pitch, stream, head, rand, None)
}

/// [`compose`], and in the HD mode (`hd`, a port addition) the pictures
/// the HD layer is made of: the picture under the body, the body's
/// pixels, the picture before the head and after it, and the head drawn
/// again at the HD size by the same pipeline ([`Head::draw_scaled`]),
/// with the control word the original draw started under. The 640x480
/// pixels are [`compose`]'s either way.
pub fn compose_with(
    out: &mut [u16],
    zbuf: &mut [u32],
    pitch: usize,
    stream: Option<&HnmStream>,
    head: Option<(&mut Head, usize)>,
    rand: &mut Rand,
    mut hd: Option<&mut DialogHd>,
) -> Option<Fpu> {
    // The HD capture needs the original's 640 x 480 frame.
    if pitch != crate::hd::SCREEN_W || out.len() != crate::hd::SCREEN_W * crate::hd::SCREEN_H {
        hd = None;
    }
    if let Some(d) = hd.as_deref_mut() {
        d.ready = false;
        d.drawn = false;
        d.pano.clear();
        d.pano.extend_from_slice(out);
    }
    zbuf.fill(0);
    if let Some(st) = stream {
        st.draw(out, pitch, out.len() / pitch.max(1));
    }
    if let Some(d) = hd.as_deref_mut() {
        d.body.resize(out.len(), 0);
        match stream {
            Some(st) => st.coverage(&mut d.body, pitch),
            None => d.body.fill(0),
        }
        d.pre_head.clear();
        d.pre_head.extend_from_slice(out);
        d.composite.clear();
        d.composite.extend_from_slice(out);
        d.ready = true;
    }
    let (h, cam) = head?;
    let before = h.fpu();
    h.display(cam, rand, out, zbuf);
    if let Some(d) = hd {
        d.composite.clear();
        d.composite.extend_from_slice(out);
        let (w, ht) = d.head_size();
        if w > 0 {
            d.head.resize(w * ht, 0);
            d.zbuf.clear();
            d.zbuf.resize(w * ht, 0);
            d.drawn = h.draw_scaled(cam, before, d.tile, &mut d.head, &mut d.zbuf, w, ht);
            // The pixels the rasteriser wrote: their Z is not 0 (a head
            // pixel's Z is 2³¹ / z, z at most the far plane).
            d.mask.clear();
            d.mask.extend(d.zbuf.iter().map(|&z| u8::from(z != 0)));
        }
    }
    Some(h.fpu())
}

/// `dlg_Perso::AddAnime(stand)` (@ 0x00461c70): `<mesh>s<stand + 1>.db`
/// with its texture map and eye maps; `None` (logged) when the file is
/// missing or unreadable.
pub fn load_head(
    files: &A2Files,
    mesh: &str,
    stand: usize,
    variant: Variant,
    fpu: Fpu,
) -> Option<Head> {
    let name = format!("{mesh}s{}.db", stand + 1);
    let Some(data) = files.read(Folder::ThreeDDial, &name) else {
        log::warn!("bool dlg_Perso::AddAnime(const long p_lStand) ERREUR: impossible de trouver le fichier {name}.");
        return None;
    };
    let scene = Scene::parse(&data)
        .map_err(|e| log::warn!("{name}: {e:?}"))
        .ok()?;
    let map = Head::texture_name(&scene)?.to_string();
    let tga = |n: &str| {
        files
            .read(Folder::ThreeDDial, n)
            .and_then(|d| Tga::parse(&d).ok())
    };
    let (yf, ymf) = Head::eye_map_names(&map);
    let open = tga(&map)?;
    Head::new(fpu, variant, &scene, open, tga(&yf), tga(&ymf))
        .map_err(|e| log::warn!("{name}: {e}"))
        .ok()
}
