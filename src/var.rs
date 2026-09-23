//! Local variable allocation: maps bytecode slots (possibly reused across
//! disjoint pc ranges) to named variables.

//! Variable table: what the front-end knows about a method's locals.
//!
//! The *data* is machine-neutral (a name and a type per variable); how it is
//! filled is not. A JVM front-end derives it from `LocalVariableTable`s; a DEX
//! front-end synthesizes it from register live ranges (release APKs usually
//! carry no local names at all).

use std::collections::HashSet;

use crate::ir::expr::TypeRef;
use crate::types::{GenericType, JavaType};

#[derive(Debug, Clone)]
pub struct VarInfo {
    pub id: u32,
    pub slot: u16,
    pub name: String,
    pub ty: TypeRef,
    pub is_param: bool,
    /// pc range where this variable is live (from LVT; method end when open).
    pub range_start: u16,
    pub range_end: u16,
    /// Set when the name was synthesized (no LVT entry).
    pub synthetic_name: bool,
}

#[derive(Debug, Clone, Default)]
pub struct VarTable {
    pub vars: Vec<VarInfo>,
    /// Per slot: (range_start, range_end, var_id), sorted by range_start.
    pub by_slot: Vec<Vec<(u16, u16, u32)>>,
    pub has_lvt: bool,
    /// Synthetic stack-merge variables (need declaration hoisting).
    pub stack_vars: Vec<u32>,
    /// Merge variables whose branches disagreed on type (or carried a
    /// generic/plain-Object value); they must stay Object so every branch
    /// assignment type-checks. Evidence inference may not narrow them.
    pub wide_stack_vars: HashSet<u32>,
    /// Variables whose declared type must render as the CONCRETE class
    /// (via `shorten_concrete`), NOT the anonymous-class → SAM-interface
    /// fallback. A `$<digits>` desugared/suspend-lambda local that owns a
    /// non-static FIELD access (`v.L$0 = obj`, the Kotlin coroutine
    /// capture fields) needs its concrete type: interfaces carry no
    /// instance fields, so `Function2 v; v.L$0` is "找不到符号 变量 L$0".
    /// Set by the front-end for single-concrete-typed field owners; the
    /// phi case is excluded because a disagreed merge stays Object (in
    /// `wide_stack_vars`), never a specific concrete class.
    pub force_concrete_vars: HashSet<u32>,
    /// Exception-table handler start pcs: an LVT range beginning at one
    /// (or at its astore + 1/2) is a catch parameter binding. Such
    /// ranges never receive forward store attribution — a store just
    /// BEFORE a handler initializes the try-body variable, not the
    /// catch param (jdk11 HostnameChecker.matchDNS: `sni = new
    /// SNIHostName(..)` at pc 8 landed on `iae` whose handler range
    /// starts at 14, within the 8-byte lead-in — SNIHostName无法转换为
    /// IllegalArgumentException).
    pub handler_starts: Vec<u16>,
}

impl VarTable {
    pub fn add_stack_var(&mut self, slot: u16, name: String, ty: TypeRef) -> u32 {
        let id = self.add(slot, name, ty, false, 0, u16::MAX, true);
        self.stack_vars.push(id);
        id
    }

    /// Allocate a synthetic variable that is NOT a stack-merge temp (e.g. a
    /// catch parameter); it must not be hoisted by the stack-var pass.
    /// Add a new variable identity for slot-reuse splitting (post-build;
    /// not registered in pc-range lookups).
    pub fn add_split(&mut self, slot: u16, name: String, ty: TypeRef) -> u32 {
        let id = self.vars.len() as u32;
        self.vars.push(VarInfo {
            id,
            slot,
            name,
            ty,
            is_param: false,
            range_start: 0,
            range_end: u16::MAX,
            synthetic_name: true,
        });
        id
    }

    pub fn add_catch_var(&mut self, slot: u16, name: String, ty: TypeRef) -> u32 {
        self.add(slot, name, ty, false, 0, u16::MAX, true)
    }

    #[allow(clippy::too_many_arguments)] // one builder for all var kinds
    fn add(
        &mut self,
        slot: u16,
        name: String,
        ty: TypeRef,
        is_param: bool,
        rs: u16,
        re: u16,
        synth: bool,
    ) -> u32 {
        let id = self.vars.len() as u32;
        self.vars.push(VarInfo {
            id,
            slot,
            name,
            ty,
            is_param,
            range_start: rs,
            range_end: re,
            synthetic_name: synth,
        });
        while self.by_slot.len() <= slot as usize {
            self.by_slot.push(Vec::new());
        }
        self.by_slot[slot as usize].push((rs, re, id));
        id
    }

    /// Resolve the variable live at (pc, slot). Falls back to the nearest
    /// preceding range, then to any var on the slot.
    pub fn at(&self, slot: u16, pc: u16) -> Option<u32> {
        let segs = self.by_slot.get(slot as usize)?;
        for (rs, re, id) in segs {
            if pc >= *rs && pc < *re {
                return Some(*id);
            }
        }
        // javac often starts an LVT range just AFTER the storing
        // instruction; attribute a nearby access to the next range before
        // falling back to the previous one (keeps slot-reuse splits like
        // `byte[] dst` / `int b` on one slot apart). Never forward-
        // attribute into a catch-param range (starts at a handler pc or
        // its astore+1/+2): the store before a handler initializes the
        // try-body variable (jdk11 HostnameChecker.matchDNS `sni = new
        // SNIHostName(..)` was typed IllegalArgumentException).
        for (rs, _, id) in segs {
            if *rs > pc
                && *rs - pc <= 8
                && !self
                    .handler_starts
                    .iter()
                    .any(|h| *rs == *h || *rs == h + 1 || *rs == h + 2)
            {
                return Some(*id);
            }
        }
        let mut best: Option<u32> = None;
        for (rs, _, id) in segs {
            if *rs <= pc {
                best = Some(*id);
            }
        }
        best.or_else(|| segs.first().map(|(_, _, id)| *id))
    }

    /// The single variable covering a slot (when the slot is not reused).
    pub fn sole_on_slot(&self, slot: u16) -> Option<u32> {
        let segs = self.by_slot.get(slot as usize)?;
        if segs.len() == 1 {
            Some(segs[0].2)
        } else {
            None
        }
    }

    pub fn vars_on_slot(&self, slot: u16) -> &[(u16, u16, u32)] {
        self.by_slot
            .get(slot as usize)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn var(&self, id: u32) -> &VarInfo {
        self.vars.get(id as usize).unwrap_or_else(|| dummy_var(id))
    }

    pub fn var_mut(&mut self, id: u32) -> &mut VarInfo {
        &mut self.vars[id as usize]
    }
}

/// Out-of-range fallback for `VarTable::var`: a PER-ID leaked dummy.
/// The previous form was a single process-wide `OnceLock<VarInfo>` —
/// initialized by whichever worker thread first hit a dangling id, so
/// EVERY dangling reference in the run printed THAT thread's id
/// (`var0`/`var1` flips across identical runs; three reqable enum
/// families flipped in lockstep). Keying by id makes the fallback
/// deterministic; leaks are bounded by distinct dangling ids per run.
fn dummy_var(id: u32) -> &'static VarInfo {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static DUMMIES: OnceLock<Mutex<HashMap<u32, &'static VarInfo>>> = OnceLock::new();
    let map = DUMMIES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut g = map.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(&v) = g.get(&id) {
        return v;
    }
    let v: &'static VarInfo = Box::leak(Box::new(VarInfo {
        id,
        slot: u16::MAX,
        name: format!("var{}", id),
        ty: TypeRef::J(crate::types::JavaType::Int),
        is_param: false,
        range_start: 0,
        range_end: 0,
        synthetic_name: true,
    }));
    g.insert(id, v);
    v
}

pub fn sig_type_at(
    lvtt: &[(u16, u16, String, u16)],
    start: u16,
    slot: u16,
    base: &JavaType,
) -> Option<TypeRef> {
    lvtt.iter()
        .find(|(s, _, _, sl)| *s == start && *sl == slot)
        .and_then(|(_, _, sig, _)| crate::types::parse_field_signature(sig).map(TypeRef::G))
        // Reject signatures that cannot describe the descriptor type (a
        // merged LVT range can accidentally align with an unrelated
        // generic entry, e.g. a type variable E over a concrete class).
        .filter(|tr| sig_matches_base(tr, base))
}

pub fn sig_matches_base(tr: &TypeRef, base: &JavaType) -> bool {
    let g = match tr {
        TypeRef::G(g) => g,
        TypeRef::J(_) => return true,
    };
    match g {
        // A type variable erases to its bound (Object or a concrete
        // class), so any reference descriptor is compatible.
        crate::types::GenericType::TypeVar(_) => matches!(base, JavaType::Object(_)),
        crate::types::GenericType::Class(cs) => {
            matches!(base, JavaType::Object(n) if n.as_ref() == cs.internal_name())
        }
        crate::types::GenericType::Primitive(c) => base.primitive_char() == Some(*c),
        crate::types::GenericType::Array(_) => matches!(base, JavaType::Array(_)),
        crate::types::GenericType::Wildcard(_) => false,
    }
}

/// Placeholder type used when the builder cannot infer a better one.
pub fn unknown_ref_type() -> TypeRef {
    TypeRef::G(GenericType::Class(crate::types::ClassSig {
        package: "java/lang".into(),
        parts: vec![crate::types::ClassSigPart {
            name: "Object".into(),
            args: vec![],
        }],
    }))
}
