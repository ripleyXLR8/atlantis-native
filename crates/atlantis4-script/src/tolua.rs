//! The 54 tolua++ bindings and their argument checks.
//!
//! atlantis4-gog-en-2363910 registers 54 `tolua_function` entries under 51
//! names as Lua globals (@ `0x00581950`); three names are tolua++
//! overloads: the later entry replaces the earlier one in the globals and
//! falls back to it when its own checks fail. Each binding first checks
//! its arguments in order (tolua++'s `tolua_is*`, mostly inlined by the
//! compiler), then `tolua_isnoobj` one past the last, then converts them
//! with `tolua_to*`. The table below was checked against the executable by
//! running every binding's checks, unmodified, in Unicorn on 185,362
//! argument tuples; its text form is the golden
//! `tests/golden/tolua_signatures.txt`.

/// One registration entry, in registration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Binding {
    Selected,
    TestObjectLimit,
    ObjectCount,
    PrintDebugMessage,
    SetCondition,
    UnsetCondition,
    TakeObject,
    PutObject,
    RemoveObject,
    StartAnimationPart,
    StartAnimation,
    AnimationSetToEnd,
    AnimationSetToStart,
    DrawText,
    ChangeWarp,
    AddToBag,
    AddToBagWithoutNotification,
    /// `RemoveFromBag()`: the selected object (first entry of the name).
    RemoveSelectedFromBag,
    /// `RemoveFromBag(name)` (second entry; falls back to the first).
    RemoveFromBag,
    SaveGame,
    SetMarker,
    LookAt,
    Wait,
    PlaySound,
    StopSound,
    PlayMusic,
    /// `ShowPuzzle(n)` (first entry of the name).
    ShowPuzzle,
    /// `ShowPuzzle(a, b, c)` (second entry; falls back to the first).
    ShowPuzzle3,
    PlayMovie,
    SetFov,
    LoadSprite,
    UnloadSprite,
    /// `PushAnswer(const TeString&)` (first entry of the name): needs a
    /// tolua++ `TeString` object, which no script can make.
    PushAnswerObject,
    FinishGame,
    ShowDocument,
    HideDocument,
    AddDocument,
    ClearImportantDocuments,
    SetImportantDocument,
    SetViewAngleXLimits,
    SetViewAngleYLimits,
    AddRandomSound,
    PlayRandomSound,
    OpenUrl,
    SetDiaryPage,
    ShowDiaryPage,
    SetInventoryAvatar,
    PuzzleTimerSetDifficulty,
    PuzzleTimerExit,
    SetDrunkFactor,
    InitDialog,
    HideDialog,
    /// `PushAnswer(id, [layout])` (second entry; falls back to the first).
    PushAnswer,
    HideAnswers,
}

/// An argument check, in order. `Opt*` checks run only when
/// `lua_gettop >= index`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    /// `nil`, a string or a number.
    Str,
    /// A number or a string `luaO_str2d` accepts.
    Num,
    /// `nil` or a boolean (tolua++'s helper with a default: a missing
    /// argument passes too).
    Bool,
    OptStr,
    OptNum,
    OptBool,
    /// `tolua_isvaluenil` then `tolua_isusertype(L, i, "const TeString")`.
    ValueTeString,
}

/// A `tolua_to*` call: argument index (1-based) and the default used when
/// `lua_gettop < index`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Conversion {
    /// `tolua_tostring`; `None` is a NULL default.
    Str(usize, Option<&'static str>),
    Num(usize, f64),
    Bool(usize, bool),
    /// `tolua_tousertype` (only `PushAnswerObject`).
    Object(usize),
}

/// One binding's signature.
#[derive(Clone, Copy, Debug)]
pub struct Signature {
    pub binding: Binding,
    /// The Lua global name.
    pub name: &'static str,
    /// The C function in atlantis4-gog-en-2363910.
    pub address: u32,
    pub checks: &'static [Check],
    pub conversions: &'static [Conversion],
    /// tolua++ overload: the entry that runs when the checks fail.
    pub fallback: Option<Binding>,
}

use Check::*;
use Conversion as C;

macro_rules! sig {
    ($b:ident, $name:literal, $addr:literal, [$($c:expr),*], [$($v:expr),*]) => {
        Signature { binding: Binding::$b, name: $name, address: $addr, checks: &[$($c),*], conversions: &[$($v),*], fallback: None }
    };
    ($b:ident, $name:literal, $addr:literal, [$($c:expr),*], [$($v:expr),*], $fb:ident) => {
        Signature { binding: Binding::$b, name: $name, address: $addr, checks: &[$($c),*], conversions: &[$($v),*], fallback: Some(Binding::$fb) }
    };
}

/// The 54 entries in registration order (atlantis4-gog-en-2363910
/// @ `0x00581950`).
#[rustfmt::skip]
pub const SIGNATURES: [Signature; 54] = [
    sig!(Selected, "Selected", 0x0057_fc30, [Str], [C::Str(1, None)]),
    sig!(TestObjectLimit, "TestObjectLimit", 0x0057_fa50, [Str, Num], [C::Str(1, None), C::Num(2, 0.0)]),
    sig!(ObjectCount, "ObjectCount", 0x0057_f8f0, [Str], [C::Str(1, None)]),
    sig!(PrintDebugMessage, "PrintDebugMessage", 0x0057_f7c0, [Str], [C::Str(1, None)]),
    sig!(SetCondition, "SetCondition", 0x0057_f6a0, [Str], [C::Str(1, None)]),
    sig!(UnsetCondition, "UnsetCondition", 0x0057_f580, [Str], [C::Str(1, None)]),
    sig!(TakeObject, "TakeObject", 0x0058_1110, [Str], [C::Str(1, None)]),
    sig!(PutObject, "PutObject", 0x0057_f410, [Str, OptBool], [C::Str(1, None), C::Bool(2, true)]),
    sig!(RemoveObject, "RemoveObject", 0x0057_f2e0, [Str], [C::Str(1, None)]),
    sig!(StartAnimationPart, "StartAnimationPart", 0x0057_f040, [Str, Num, Num, OptNum, OptBool],
        [C::Str(1, None), C::Num(2, 0.0), C::Num(3, 0.0), C::Num(4, -1.0), C::Bool(5, false)]),
    sig!(StartAnimation, "StartAnimation", 0x0057_ee40, [Str, OptNum, OptBool], [C::Str(1, None), C::Num(2, -1.0), C::Bool(3, false)]),
    sig!(AnimationSetToEnd, "AnimationSetToEnd", 0x0057_ece0, [Str, OptBool], [C::Str(1, None), C::Bool(2, false)]),
    sig!(AnimationSetToStart, "AnimationSetToStart", 0x0057_eb80, [Str, OptBool], [C::Str(1, None), C::Bool(2, false)]),
    sig!(DrawText, "DrawText", 0x0058_01d0, [Str, Str, OptNum, OptStr, OptStr],
        [C::Str(1, None), C::Str(2, None), C::Num(3, 0.0), C::Str(4, Some("")), C::Str(5, Some(""))]),
    sig!(ChangeWarp, "ChangeWarp", 0x0058_0840, [Str, OptBool], [C::Str(1, None), C::Bool(2, true)]),
    sig!(AddToBag, "AddToBag", 0x0058_0fc0, [Str], [C::Str(1, None)]),
    sig!(AddToBagWithoutNotification, "AddToBagWithoutNotification", 0x0058_0e70, [Str], [C::Str(1, None)]),
    sig!(RemoveSelectedFromBag, "RemoveFromBag", 0x0057_eb00, [], []),
    sig!(RemoveFromBag, "RemoveFromBag", 0x0058_00d0, [Str], [C::Str(1, None)], RemoveSelectedFromBag),
    sig!(SaveGame, "SaveGame", 0x0057_e9d0, [Str], [C::Str(1, None)]),
    sig!(SetMarker, "SetMarker", 0x0057_e7a0, [Str, Num, OptNum], [C::Str(1, None), C::Num(2, 0.0), C::Num(3, 0.0)]),
    sig!(LookAt, "LookAt", 0x0057_e680, [Num, Num], [C::Num(1, 0.0), C::Num(2, 0.0)]),
    sig!(Wait, "Wait", 0x0057_e550, [Num], [C::Num(1, 0.0)]),
    sig!(PlaySound, "PlaySound", 0x0057_ff30, [Str, Num], [C::Str(1, None), C::Num(2, 0.0)]),
    sig!(StopSound, "StopSound", 0x0057_e420, [Str], [C::Str(1, None)]),
    sig!(PlayMusic, "PlayMusic", 0x0057_e300, [Str], [C::Str(1, None)]),
    sig!(ShowPuzzle, "ShowPuzzle", 0x0058_0d60, [Num], [C::Num(1, 0.0)]),
    sig!(ShowPuzzle3, "ShowPuzzle", 0x0058_17b0, [Num, Num, Num], [C::Num(1, 0.0), C::Num(2, 0.0), C::Num(3, 0.0)], ShowPuzzle),
    sig!(PlayMovie, "PlayMovie", 0x0057_e160, [Str, Str], [C::Str(1, None), C::Str(2, None)]),
    sig!(SetFov, "SetFOV", 0x0057_e080, [Num], [C::Num(1, 0.0)]),
    sig!(LoadSprite, "LoadSprite", 0x0058_0c30, [Str], [C::Str(1, None)]),
    sig!(UnloadSprite, "UnloadSprite", 0x0058_06a0, [Str], [C::Str(1, None)]),
    sig!(PushAnswerObject, "PushAnswer", 0x0058_0ad0, [ValueTeString], [C::Object(1)]),
    sig!(FinishGame, "FinishGame", 0x0058_07e0, [], []),
    sig!(ShowDocument, "ShowDocument", 0x0058_09a0, [Str], [C::Str(1, None)]),
    sig!(HideDocument, "HideDocument", 0x0058_0620, [], []),
    sig!(AddDocument, "AddDocument", 0x0058_1680, [Str], [C::Str(1, None)]),
    sig!(ClearImportantDocuments, "ClearImportantDocuments", 0x0057_e030, [], []),
    sig!(SetImportantDocument, "SetImportantDocument", 0x0057_df60, [Str], [C::Str(1, None)]),
    sig!(SetViewAngleXLimits, "SetViewAngleXLimits", 0x0057_de30, [Num, Num], [C::Num(1, 0.0), C::Num(2, 0.0)]),
    sig!(SetViewAngleYLimits, "SetViewAngleYLimits", 0x0057_dd00, [Num, Num], [C::Num(1, 0.0), C::Num(2, 0.0)]),
    sig!(AddRandomSound, "AddRandomSound", 0x0057_dae0, [Str, Str, Num], [C::Str(1, None), C::Str(2, None), C::Num(3, 0.0)]),
    sig!(PlayRandomSound, "PlayRandomSound", 0x0057_d9b0, [Str], [C::Str(1, None)]),
    sig!(OpenUrl, "OpenURL", 0x0057_d870, [Str], [C::Str(1, None)]),
    sig!(SetDiaryPage, "SetDiaryPage", 0x0058_14c0, [Num, Str], [C::Num(1, 0.0), C::Str(2, None)]),
    sig!(ShowDiaryPage, "ShowDiaryPage", 0x0058_13a0, [Num], [C::Num(1, 0.0)]),
    sig!(SetInventoryAvatar, "SetInventoryAvatar", 0x0057_d740, [Str], [C::Str(1, None)]),
    sig!(PuzzleTimerSetDifficulty, "PuzzleTimerSetDifficulty", 0x0057_d650, [Num], [C::Num(1, 0.0)]),
    sig!(PuzzleTimerExit, "PuzzleTimerExit", 0x0058_0580, [], []),
    sig!(SetDrunkFactor, "SetDrunkFactor", 0x0057_d510, [Num, Num], [C::Num(1, 0.0), C::Num(2, 0.0)]),
    sig!(InitDialog, "InitDialog", 0x0057_fd70, [Str, OptStr], [C::Str(1, None), C::Str(2, Some(""))]),
    sig!(HideDialog, "HideDialog", 0x0057_d490, [], []),
    sig!(PushAnswer, "PushAnswer", 0x0058_1220, [Str, OptStr], [C::Str(1, None), C::Str(2, Some("GUI/answer.lua"))], PushAnswerObject),
    sig!(HideAnswers, "HideAnswers", 0x0058_0500, [], []),
];

impl Binding {
    /// The signature of this entry.
    pub fn signature(self) -> &'static Signature {
        &SIGNATURES[self as usize]
    }

    /// The Lua global name.
    pub fn name(self) -> &'static str {
        self.signature().name
    }
}

impl Signature {
    /// The signature in the golden file's line format (see
    /// `tests/golden/tolua_signatures.txt`).
    pub fn render(&self) -> String {
        let mut s = format!("{} 0x{:x} ", self.name, self.address);
        if self.checks.is_empty() {
            s.push('-');
        }
        for (i, c) in self.checks.iter().enumerate() {
            if i > 0 {
                s.push(' ');
            }
            s.push_str(match c {
                Str => "s",
                Num => "n",
                Bool => "b",
                OptStr => "s?",
                OptNum => "n?",
                OptBool => "b?",
                ValueTeString => "V",
            });
        }
        s.push_str(" -> ");
        if self.conversions.is_empty() {
            s.push('-');
        }
        for (i, c) in self.conversions.iter().enumerate() {
            if i > 0 {
                s.push(' ');
            }
            match c {
                C::Str(i, None) => s.push_str(&format!("s{i}:NULL")),
                C::Str(i, Some(d)) => s.push_str(&format!("s{i}:\"{d}\"")),
                C::Num(i, d) => s.push_str(&format!("n{i}:{d}")),
                C::Bool(i, d) => s.push_str(&format!("b{i}:{}", u8::from(*d))),
                C::Object(i) => s.push_str(&format!("u{i}:NULL")),
            }
        }
        if let Some(fb) = self.fallback {
            s.push_str(&format!(" fallback=0x{:x}", fb.signature().address));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_in_registration_order() {
        for (i, s) in SIGNATURES.iter().enumerate() {
            assert_eq!(s.binding as usize, i, "{}", s.name);
        }
    }
}
