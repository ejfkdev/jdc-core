//! A fast, non-cryptographic hasher for compiler-internal maps keyed by
//! block/variable indices (the rustc "FxHash" scheme).
//!
//! The decompiler builds and probes millions of tiny `HashSet<usize>` /
//! `HashMap<usize, _>` universes per run (dominators, RPO, region walks).
//! SipHash-1-3 (std default) costs ~15-20ns per probe; the rotate-xor-
//! multiply mix below costs ~1ns and is what rustc itself uses for
//! exactly this key shape. NOT for adversarial-input-facing tables.

use std::hash::{BuildHasherDefault, Hasher};

#[cfg(target_pointer_width = "64")]
const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;
#[cfg(target_pointer_width = "32")]
const SEED: u64 = 0x9e_37_79_b9;

#[derive(Default, Clone, Copy, Debug)]
pub struct FxHasher {
    hash: u64,
}

impl FxHasher {
    #[inline]
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(SEED);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let mut b = bytes;
        while b.len() >= 8 {
            let mut buf = [0u8; 8];
            buf.copy_from_slice(&b[..8]);
            self.add(u64::from_le_bytes(buf));
            b = &b[8..];
        }
        if b.len() >= 4 {
            let mut buf = [0u8; 4];
            buf.copy_from_slice(&b[..4]);
            self.add(u32::from_le_bytes(buf) as u64);
            b = &b[4..];
        }
        for &byte in b {
            self.add(byte as u64);
        }
    }
    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.add(i as u64);
    }
    #[inline]
    fn write_u16(&mut self, i: u16) {
        self.add(i as u64);
    }
    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.add(i as u64);
    }
    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }
    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }
    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}

pub type FxBuildHasher = BuildHasherDefault<FxHasher>;
pub type FxHashMap<K, V> = std::collections::HashMap<K, V, FxBuildHasher>;
pub type FxHashSet<T> = std::collections::HashSet<T, FxBuildHasher>;
