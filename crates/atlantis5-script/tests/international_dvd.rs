//! The international DVD's scripts, Lua 5.0 source (`ATLANTIS5_DVD_INT`:
//! a copy of the disc's files; skipped when unset), compiled as rilua
//! compiles them for the game (its Native patch 11: Lua 5.0's loops).
//!
//! With `ATLANTIS5_DVD_FR` too (the French DVD's extracted cabinets, whose
//! 66 chunks Lua 5.0's `luac` compiled from these sources line for line):
//! every function of every script has the same loop instructions as the
//! French chunk's, on the same lines, in the same order: `FORLOOP`,
//! `TFORPREP` and `TFORLOOP` as Lua 5.0 codes them, no 5.1 `FORPREP`.

use atlantis3_data::atlantis5::Atlantis5Source;
use atlantis5_script::rilua::compiler::codegen::compile_lua50;
use atlantis5_script::rilua::vm::instructions::{Instruction, OpCode};
use atlantis5_script::rilua::vm::undump50::undump50;
use atlantis5_script::rilua::ProtoRef;
use atlantis_data::FileSource;

/// The scripts' paths below `datas/scripts/` (lower case).
fn scripts(src: &Atlantis5Source) -> Vec<String> {
    let mut out = Vec::new();
    let mut dirs = vec![String::new()];
    while let Some(d) = dirs.pop() {
        for n in src.list(&format!("datas/scripts/{d}")) {
            let p = format!("{d}{}", n.to_ascii_lowercase());
            if p.ends_with(".lua") {
                out.push(p);
            } else {
                dirs.push(format!("{p}/"));
            }
        }
    }
    out.sort();
    out
}

/// Each function's loop instructions (name, line), the functions in
/// order (each before its nested ones).
fn loops(p: &ProtoRef, out: &mut Vec<Vec<(&'static str, u32)>>) {
    let mut mine = Vec::new();
    for (pc, raw) in p.code.iter().enumerate() {
        let op = Instruction::from_raw(*raw).opcode();
        if matches!(
            op,
            OpCode::ForLoop50
                | OpCode::TForPrep50
                | OpCode::TForLoop50
                | OpCode::ForPrep
                | OpCode::ForLoop
                | OpCode::TForLoop
        ) {
            mine.push((op.name(), p.line_info.get(pc).copied().unwrap_or(0)));
        }
    }
    out.push(mine);
    for c in &p.protos {
        loops(c, out);
    }
}

#[test]
fn the_scripts_compile_with_lua_50s_loops() {
    let Some(d) = atlantis3_data::testing::a5_dvd_int() else {
        return;
    };
    let int = Atlantis5Source::open(&d).expect("the disc");
    let names = scripts(&int);
    assert_eq!(names.len(), 66);
    let fr = std::env::var_os("ATLANTIS5_DVD_FR")
        .map(|f| Atlantis5Source::open(&std::path::PathBuf::from(f).join("datas")).expect("tree"));
    if fr.is_none() {
        eprintln!("ATLANTIS5_DVD_FR not set: the French chunks are not compared");
    }
    let (mut functions, mut loop_ops) = (0, 0);
    for n in &names {
        let path = format!("datas/scripts/{n}");
        let source = int.read(&path).expect("the source");
        assert!(!source.starts_with(b"\x1bLua"), "{n}: source");
        let p = compile_lua50(&source, &format!("@./src/{n}"), true)
            .unwrap_or_else(|e| panic!("{n}: {e}"));
        let mut ours = Vec::new();
        loops(&p, &mut ours);
        assert!(
            ours.iter().flatten().all(|(op, _)| op.ends_with("50")),
            "{n}: a 5.1 loop {ours:?}"
        );
        functions += ours.len();
        loop_ops += ours.iter().map(Vec::len).sum::<usize>();
        if let Some(fr) = &fr {
            let chunk = fr.read(&path).expect("the French chunk");
            let q = undump50(&chunk, n).unwrap_or_else(|e| panic!("{n}: {e}"));
            let mut theirs = Vec::new();
            loops(&q, &mut theirs);
            assert_eq!(ours, theirs, "{n}");
        }
    }
    eprintln!("{functions} functions, {loop_ops} loop instructions");
    assert!(loop_ops > 0);
}

/// The North-American CDs' scripts (`ATLANTIS5_CD_US`, read from the
/// discs' InstallShield set; skipped when unset): the international
/// DVD's 66 sources, compiled the same way.
#[test]
fn the_north_american_cds_scripts_compile_too() {
    let Some(d) = atlantis3_data::testing::a5_cd_us() else {
        return;
    };
    let us = Atlantis5Source::open(&d).expect("the discs");
    let names = scripts(&us);
    assert_eq!(names.len(), 66);
    let int =
        atlantis3_data::testing::a5_dvd_int().map(|d| Atlantis5Source::open(&d).expect("the DVD"));
    for n in &names {
        let path = format!("datas/scripts/{n}");
        let source = us.read(&path).expect("the source");
        assert!(!source.starts_with(b"\x1bLua"), "{n}: source");
        compile_lua50(&source, &format!("@./src/{n}"), true).unwrap_or_else(|e| panic!("{n}: {e}"));
        if let Some(int) = &int {
            assert_eq!(int.read(&path).as_deref(), Some(&source[..]), "{n}");
        }
    }
}
