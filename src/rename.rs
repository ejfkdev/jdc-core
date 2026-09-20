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
