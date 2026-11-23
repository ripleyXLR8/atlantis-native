//! Lua 5.0 binary chunks (Native patch 4).
//!
//! Reads a chunk in Lua 5.0's `lundump.c` format (5.0.2/5.0.3: header
//! `ESC "Lua" 0x50`, endianness, the sizes of int, size_t, Instruction,
//! the OP/A/B/C field widths 6/8/9/9 and lua_Number, a test number) and
//! gives a [`Proto`] that this VM runs with Lua 5.0's meaning:
//!
//! - each instruction is re-encoded from 5.0's layout (OP bits 0-5, C
//!   6-14, B 15-23, A 24-31; Bx = bits 6-23) into 5.1's;
//! - 5.0's RK operand (`x < 250`: register, else constant `x - 250`)
//!   becomes 5.1's (bit 256 marks a constant); a function whose RK
//!   operands name a constant past 255 gets its constants reordered, those
//!   used as RK first (the order of a constant pool is not visible);
//! - the operations that 5.1 has with the same meaning keep their 5.1
//!   code (5.0 `TEST` is 5.1 `TESTSET`); the others (`FORLOOP`,
//!   `TFORLOOP`, `TFORPREP`, `SETLIST`, `SETLISTO`, `NEWTABLE`) get the
//!   `*50` codes, run as Lua 5.0.3's `lvm.c` runs them;
//! - `POW` is 5.1's: 5.0 calls the global `__pow`, which `luaopen_math`
//!   sets to `math.pow`, so the two agree on numbers.
//!
//! Constants are nil, numbers and strings (5.0 has no boolean constant).
//! A 5.0 vararg function (`arg` table) gets 5.1's compatibility flags.

use crate::error::{LuaError, LuaResult, SyntaxError};

use super::instructions::{Instruction, OpCode};
use super::proto::{LocalVar, Proto, ProtoRef, VARARG_HASARG, VARARG_ISVARARG, VARARG_NEEDSARG};
use super::value::Val;

/// `ESC "Lua"` followed by the version byte 0x50.
pub const LUA50_SIGNATURE: &[u8] = b"\x1bLua\x50";

/// 5.0's `MAXSTACK`: an RK operand at or above it names a constant.
const MAXSTACK50: u32 = 250;

/// Lua 5.0's opcodes in their order (`lopcodes.h`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op50 {
    Move,
    LoadK,
    LoadBool,
    LoadNil,
    GetUpval,
    GetGlobal,
    GetTable,
    SetGlobal,
    SetUpval,
    SetTable,
    NewTable,
    OpSelf,
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Unm,
    Not,
    Concat,
    Jmp,
    Eq,
    Lt,
    Le,
    Test,
    Call,
    TailCall,
    Return,
    ForLoop,
    TForLoop,
    TForPrep,
    SetList,
    SetListO,
    Close,
    Closure,
}

const OPS50: [Op50; 35] = [
    Op50::Move,
    Op50::LoadK,
    Op50::LoadBool,
    Op50::LoadNil,
    Op50::GetUpval,
    Op50::GetGlobal,
    Op50::GetTable,
    Op50::SetGlobal,
    Op50::SetUpval,
    Op50::SetTable,
    Op50::NewTable,
    Op50::OpSelf,
    Op50::Add,
    Op50::Sub,
    Op50::Mul,
    Op50::Div,
    Op50::Pow,
    Op50::Unm,
    Op50::Not,
    Op50::Concat,
    Op50::Jmp,
    Op50::Eq,
    Op50::Lt,
    Op50::Le,
    Op50::Test,
    Op50::Call,
    Op50::TailCall,
    Op50::Return,
    Op50::ForLoop,
    Op50::TForLoop,
    Op50::TForPrep,
    Op50::SetList,
    Op50::SetListO,
    Op50::Close,
    Op50::Closure,
];

/// A 5.0 instruction's fields.
#[derive(Clone, Copy)]
struct Ins50 {
    op: Op50,
    a: u32,
    b: u32,
    c: u32,
    bx: u32,
}

impl Ins50 {
    fn decode(i: u32) -> Option<Self> {
        let op = *OPS50.get((i & 0x3f) as usize)?;
        Some(Self {
            op,
            c: (i >> 6) & 0x1ff,
            b: (i >> 15) & 0x1ff,
            a: i >> 24,
            bx: (i >> 6) & 0x3ffff,
        })
    }

    /// The operands that are RK in 5.0 (B, C).
    fn rk_operands(self) -> (bool, bool) {
        match self.op {
            Op50::GetTable | Op50::OpSelf => (false, true),
            Op50::SetTable
            | Op50::Add
            | Op50::Sub
            | Op50::Mul
            | Op50::Div
            | Op50::Pow
            | Op50::Eq
            | Op50::Lt
            | Op50::Le => (true, true),
            _ => (false, false),
        }
    }
}

struct Loader<'a> {
    data: &'a [u8],
    pos: usize,
    name: String,
    size_t: usize,
    instruction: usize,
}

impl Loader<'_> {
    fn error(&self, reason: &str) -> LuaError {
        LuaError::Syntax(SyntaxError {
            message: format!("{}: {} in precompiled chunk", self.name, reason),
            source: self.name.clone(),
            line: 0,
            raw_message: None,
        })
    }

    fn block(&mut self, n: usize) -> LuaResult<&[u8]> {
        if self.pos + n > self.data.len() {
            return Err(self.error("truncated"));
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn byte(&mut self) -> LuaResult<u8> {
        Ok(self.block(1)?[0])
    }

    fn int(&mut self) -> LuaResult<i32> {
        let b = self.block(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn count(&mut self) -> LuaResult<usize> {
        let n = self.int()?;
        usize::try_from(n).map_err(|_| self.error("bad count"))
    }

    fn size(&mut self) -> LuaResult<u64> {
        let w = self.size_t;
        let b = self.block(w)?;
        let mut v = [0u8; 8];
        v[..w].copy_from_slice(b);
        Ok(u64::from_le_bytes(v))
    }

    fn number(&mut self) -> LuaResult<f64> {
        let b = self.block(8)?;
        let mut v = [0u8; 8];
        v.copy_from_slice(b);
        Ok(f64::from_le_bytes(v))
    }

    fn string(&mut self) -> LuaResult<Option<Vec<u8>>> {
        let n = self.size()?;
        if n == 0 {
            return Ok(None);
        }
        let n = usize::try_from(n).map_err(|_| self.error("bad string"))?;
        let s = self.block(n)?;
        Ok(Some(s[..n - 1].to_vec()))
    }

    fn instruction(&mut self) -> LuaResult<u32> {
        let w = self.instruction;
        let b = self.block(w)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn header(&mut self) -> LuaResult<()> {
        if self.block(5)? != LUA50_SIGNATURE {
            return Err(self.error("not a Lua 5.0 chunk"));
        }
        if self.byte()? != 1 {
            return Err(self.error("big-endian chunk"));
        }
        let int = self.byte()?;
        self.size_t = usize::from(self.byte()?);
        self.instruction = usize::from(self.byte()?);
        let fields = self.block(4)?;
        let ok_fields = fields == [6, 8, 9, 9];
        let number = self.byte()?;
        if int != 4
            || !matches!(self.size_t, 4 | 8)
            || !matches!(self.instruction, 4 | 8)
            || !ok_fields
            || number != 8
        {
            return Err(self.error("virtual machine mismatch"));
        }
        // TEST_NUMBER, 3.14159265358979323846E7, compared as a long.
        #[allow(clippy::cast_possible_truncation)]
        if self.number()? as i64 != 31_415_926 {
            return Err(self.error("unknown number format"));
        }
        Ok(())
    }

    fn function(&mut self, parent_source: &str) -> LuaResult<ProtoRef> {
        let source = match self.string()? {
            Some(b) => String::from_utf8_lossy(&b).into_owned(),
            None => parent_source.to_string(),
        };
        let mut f = Proto::new(&source);
        let line = self.int()?;
        f.line_defined = u32::try_from(line).unwrap_or(0);
        f.last_line_defined = f.line_defined;
        f.num_upvalues = self.byte()?;
        f.num_params = self.byte()?;
        let vararg = self.byte()?;
        f.is_vararg = if vararg != 0 {
            VARARG_HASARG | VARARG_ISVARARG | VARARG_NEEDSARG
        } else {
            0
        };
        f.max_stack_size = self.byte()?;
        let n = self.count()?;
        for _ in 0..n {
            f.line_info.push(u32::try_from(self.int()?).unwrap_or(0));
        }
        let n = self.count()?;
        for _ in 0..n {
            let name = self.string()?.unwrap_or_default();
            let start_pc = u32::try_from(self.int()?).unwrap_or(0);
            let end_pc = u32::try_from(self.int()?).unwrap_or(0);
            f.local_vars.push(LocalVar {
                name: String::from_utf8_lossy(&name).into_owned(),
                start_pc,
                end_pc,
            });
        }
        let n = self.count()?;
        for _ in 0..n {
            let name = self.string()?.unwrap_or_default();
            f.upvalue_names
                .push(String::from_utf8_lossy(&name).into_owned());
        }
        // Constants: (tag, value) in the chunk's order.
        let n = self.count()?;
        let mut consts: Vec<Konst> = Vec::with_capacity(n);
        for _ in 0..n {
            match self.byte()? {
                0 => consts.push(Konst::Nil),
                3 => consts.push(Konst::Num(self.number()?)),
                4 => consts.push(Konst::Str(self.string()?.unwrap_or_default())),
                t => return Err(self.error(&format!("bad constant type ({t})"))),
            }
        }
        let n = self.count()?;
        for _ in 0..n {
            let child = self.function(&source)?;
            f.protos.push(child);
        }
        let n = self.count()?;
        let mut code = Vec::with_capacity(n);
        for _ in 0..n {
            code.push(self.instruction()?);
        }
        let order = self.constant_order(&code, consts.len())?;
        // `order[new] = old`; `place[old] = new`.
        let mut place = vec![0u32; consts.len()];
        for (new, &old) in order.iter().enumerate() {
            place[old] = u32::try_from(new).unwrap_or(0);
        }
        for (new, &old) in order.iter().enumerate() {
            match &consts[old] {
                Konst::Nil => f.constants.push(Val::Nil),
                Konst::Num(x) => f.constants.push(Val::Num(*x)),
                Konst::Str(s) => {
                    f.constants.push(Val::Nil);
                    f.string_pool
                        .push((u32::try_from(new).unwrap_or(0), s.clone()));
                }
            }
        }
        for (pc, &raw) in code.iter().enumerate() {
            let ins = Ins50::decode(raw).ok_or_else(|| self.error("bad code"))?;
            f.code.push(
                translate(ins, &place)
                    .map_err(|e| self.error(&format!("{e} at instruction {}", pc + 1)))?,
            );
        }
        Ok(ProtoRef::new(f))
    }

    /// The constants' new order: unchanged unless an RK operand names a
    /// constant past 255; then those named by RK operands come first.
    fn constant_order(&self, code: &[u32], n: usize) -> LuaResult<Vec<usize>> {
        let mut rk_used = vec![false; n];
        let mut overflow = false;
        for &raw in code {
            let Some(ins) = Ins50::decode(raw) else {
                return Err(self.error("bad code"));
            };
            let (rb, rc) = ins.rk_operands();
            for (is_rk, x) in [(rb, ins.b), (rc, ins.c)] {
                if is_rk && x >= MAXSTACK50 {
                    let k = (x - MAXSTACK50) as usize;
                    if k >= n {
                        return Err(self.error("bad constant index"));
                    }
                    rk_used[k] = true;
                    overflow |= k > 255;
                }
            }
        }
        let mut order: Vec<usize> = (0..n).collect();
        if overflow {
            order.sort_by_key(|&k| !rk_used[k]);
            if order.iter().take_while(|&&k| rk_used[k]).count() > 256 {
                return Err(self.error("too many constants in RK operands"));
            }
        }
        Ok(order)
    }
}

enum Konst {
    Nil,
    Num(f64),
    Str(Vec<u8>),
}

/// One 5.0 instruction in this VM's encoding.
fn translate(i: Ins50, place: &[u32]) -> Result<u32, &'static str> {
    let rk = |x: u32| -> Result<u32, &'static str> {
        if x < MAXSTACK50 {
            Ok(x)
        } else {
            let k = *place
                .get((x - MAXSTACK50) as usize)
                .ok_or("bad constant index")?;
            Ok(k | 256)
        }
    };
    let k = |bx: u32| -> Result<u32, &'static str> {
        place.get(bx as usize).copied().ok_or("bad constant index")
    };
    #[allow(clippy::cast_possible_wrap)]
    let sbx = i.bx as i32 - 131_071;
    let (a, b, c) = (i.a, i.b, i.c);
    let abc = |op: OpCode, b: u32, c: u32| Instruction::abc(op, a, b, c).raw();
    Ok(match i.op {
        Op50::Move => abc(OpCode::Move, b, c),
        Op50::LoadK => Instruction::a_bx(OpCode::LoadK, a, k(i.bx)?).raw(),
        Op50::LoadBool => abc(OpCode::LoadBool, b, c),
        Op50::LoadNil => abc(OpCode::LoadNil, b, c),
        Op50::GetUpval => abc(OpCode::GetUpval, b, c),
        Op50::GetGlobal => Instruction::a_bx(OpCode::GetGlobal, a, k(i.bx)?).raw(),
        Op50::GetTable => abc(OpCode::GetTable, b, rk(c)?),
        Op50::SetGlobal => Instruction::a_bx(OpCode::SetGlobal, a, k(i.bx)?).raw(),
        Op50::SetUpval => abc(OpCode::SetUpval, b, c),
        Op50::SetTable => abc(OpCode::SetTable, rk(b)?, rk(c)?),
        Op50::NewTable => abc(OpCode::NewTable50, b, c),
        Op50::OpSelf => abc(OpCode::OpSelf, b, rk(c)?),
        Op50::Add => abc(OpCode::Add, rk(b)?, rk(c)?),
        Op50::Sub => abc(OpCode::Sub, rk(b)?, rk(c)?),
        Op50::Mul => abc(OpCode::Mul, rk(b)?, rk(c)?),
        Op50::Div => abc(OpCode::Div, rk(b)?, rk(c)?),
        Op50::Pow => abc(OpCode::Pow, rk(b)?, rk(c)?),
        Op50::Unm => abc(OpCode::Unm, b, c),
        Op50::Not => abc(OpCode::Not, b, c),
        Op50::Concat => abc(OpCode::Concat, b, c),
        Op50::Jmp => Instruction::a_sbx(OpCode::Jmp, a, sbx).raw(),
        Op50::Eq => abc(OpCode::Eq, rk(b)?, rk(c)?),
        Op50::Lt => abc(OpCode::Lt, rk(b)?, rk(c)?),
        Op50::Le => abc(OpCode::Le, rk(b)?, rk(c)?),
        // 5.0 TEST: if (R(B) <=> C) then R(A) := R(B) else pc++ = 5.1 TESTSET.
        Op50::Test => abc(OpCode::TestSet, b, c),
        Op50::Call => abc(OpCode::Call, b, c),
        Op50::TailCall => abc(OpCode::TailCall, b, c),
        Op50::Return => abc(OpCode::Return, b, c),
        Op50::ForLoop => Instruction::a_sbx(OpCode::ForLoop50, a, sbx).raw(),
        Op50::TForLoop => abc(OpCode::TForLoop50, b, c),
        Op50::TForPrep => Instruction::a_sbx(OpCode::TForPrep50, a, sbx).raw(),
        Op50::SetList => Instruction::a_bx(OpCode::SetList50, a, i.bx).raw(),
        Op50::SetListO => Instruction::a_bx(OpCode::SetListO50, a, i.bx).raw(),
        Op50::Close => abc(OpCode::Close, b, c),
        Op50::Closure => Instruction::a_bx(OpCode::Closure, a, i.bx).raw(),
    })
}

/// Loads a Lua 5.0 binary chunk (`name` as for [`super::undump::undump`]).
///
/// # Errors
///
/// `LuaError::Syntax` for a chunk that is not a little-endian Lua 5.0
/// chunk with 4-byte ints and 8-byte numbers, or is cut short.
pub fn undump50(data: &[u8], name: &str) -> LuaResult<ProtoRef> {
    let mut l = Loader {
        data,
        pos: 0,
        name: name.to_string(),
        size_t: 4,
        instruction: 4,
    };
    l.header()?;
    l.function(name)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn decodes_the_50_layout() {
        // ADD A=3 B=250(K0) C=1, 5.0 layout: OP 0-5, C 6-14, B 15-23, A 24-31.
        let raw = 12 | (1 << 6) | (250 << 15) | (3 << 24);
        let i = Ins50::decode(raw).unwrap();
        assert_eq!(i.op, Op50::Add);
        let t = Instruction::from_raw(translate(i, &[0]).unwrap());
        assert_eq!(t.opcode(), OpCode::Add);
        assert_eq!((t.a(), t.b(), t.c()), (3, 256, 1));
    }

    #[test]
    fn rejects_other_versions() {
        assert!(undump50(b"\x1bLua\x51", "=t").is_err());
    }
}
