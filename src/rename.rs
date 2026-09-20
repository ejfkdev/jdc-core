//! Case-collision class renames. Obfuscators emit class pairs differing
//! only in letter case (`X/CUA` vs `X/Cua`); on case-insensitive
//! filesystems both map to one physical file — one class's output was
//! lost. The front-end driver computes a deterministic rename (suffix
//! `_2`, `_3`, … on the simple name, first-in-sorted-group unchanged)
//! and installs it here before workers spawn; declarations, references
//! and file names then all agree.

use std::borrow::Cow;
use crate::fx::FxHashMap as HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// internal → display. Entries whose value equals the key mark
/// file-level classes that were NOT renamed (they anchor the prefix
/// walk below).
static RENAMES: OnceLock<HashMap<String, String>> = OnceLock::new();
static REVERSE: OnceLock<HashMap<String, String>> = OnceLock::new();

/// Field display renames: owner → list of (name, type descriptor, display).
/// Obfuscators rename a synthetic outer-reference field (`this$0`) to a
/// one-char name that COLLIDES with a real field of the same class
/// (`final a a;` beside `private Runnable a;`) — legal in bytecode (fields
/// resolve by index), illegal in source. Every consumer of a field name —
/// the declaration and every reference — funnels through here, so one
/// registry keeps all sites consistent. Vec per owner: a class has a
/// handful of collisions at most, and the borrow-based lookup avoids
/// allocation on the (rare) collision-hit path.
static FIELD_RENAMES: OnceLock<HashMap<std::sync::Arc<str>, Vec<FieldRename>>> = OnceLock::new();
static FIELD_ACTIVE: AtomicBool = AtomicBool::new(false);

/// One renamed field of one owner class.
pub struct FieldRename {
    pub name: std::sync::Arc<str>,
    pub desc: std::sync::Arc<str>,
    pub display: std::sync::Arc<str>,
}

/// Install the field rename registry (empty map = inactive, one relaxed
/// atomic read per lookup on clean corpora).
pub fn set_field_renames(map: HashMap<std::sync::Arc<str>, Vec<FieldRename>>) {
    if !map.is_empty() {
        FIELD_ACTIVE.store(true, Ordering::Relaxed);
    }
    let _ = FIELD_RENAMES.set(map);
}

/// Fast probe: is the member-rename registry non-empty? Callers on hot
/// paths (every method/field reference) skip descriptor reconstruction
/// entirely when it is not.
#[inline]
pub fn member_rename_active() -> bool {
    FIELD_ACTIVE.load(Ordering::Relaxed)
}

/// Display name for a field, when it was renamed.
#[inline]
pub fn field_display(owner: &str, name: &str, desc: &str) -> Option<&'static str> {
    if !FIELD_ACTIVE.load(Ordering::Relaxed) {
        return None;
    }
    FIELD_RENAMES
        .get()?
        .get(owner)?
        .iter()
        .find(|fr| &*fr.name == name && &*fr.desc == desc)
        .map(|fr| fr.display.as_ref())
}
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Install the map (call once, before worker threads spawn). Identity
/// entries for unrenamed file-level classes are expected.
pub fn set_class_renames(map: HashMap<String, String>) {
    if !map.is_empty() {
        ACTIVE.store(true, Ordering::Relaxed);
    }
    // Reverse index of real (non-identity) renames: a display name's
    // `$`-prefixes must resolve for nested references (`x0$a$a2$a3` —
    // the `x0$a$a2` prefix names no pool class, it is the display of
    // one).
    let rev: HashMap<String, String> = map
        .iter()
        .filter(|(k, v)| k != v)
        .map(|(k, v)| (v.clone(), k.clone()))
        .collect();
    let _ = REVERSE.set(rev);
    let _ = RENAMES.set(map);
}

/// True when `internal` is the DISPLAY form of a renamed class (the
/// value side of a real rename). print_class_name uses this to keep
/// dotting through renamed nesting prefixes.
#[inline]
pub fn is_renamed_display(internal: &str) -> bool {
    REVERSE.get().is_some_and(|m| m.contains_key(internal))
}

/// The display form of an internal class name. Exact file-level match
/// first; otherwise walk `$` boundaries from the right — a nested
/// reference (`X/Cua$Inner`) follows its renamed file-level owner, and
/// an unrenamed file-level prefix anchors the walk (identity).
#[inline]
pub fn apply_class_rename(internal: &str) -> Cow<'_, str> {
    if !ACTIVE.load(Ordering::Relaxed) {
        return Cow::Borrowed(internal);
    }
    let Some(map) = RENAMES.get() else {
        return Cow::Borrowed(internal);
    };
    if let Some(d) = map.get(internal) {
        return if d == internal {
            Cow::Borrowed(internal)
        } else {
            Cow::Owned(d.clone())
        };
    }
    let mut s = internal;
    while let Some(i) = s.rfind('$') {
        s = &s[..i];
        match map.get(s) {
            Some(d) if d != s => {
                let suffix = &internal[s.len()..];
                return Cow::Owned(format!("{d}{suffix}"));
            }
            // An unrenamed file-level prefix owns this name: identity.
            Some(_) => return Cow::Borrowed(internal),
            None => continue,
        }
    }
    Cow::Borrowed(internal)
}
