use std::sync::atomic::{AtomicU64, Ordering};

use crate::moves::Move;
use crate::search::{MATE, Score};

/// Furthest ply from root we expect; scores within this band of `±MATE`
/// are treated as mates and stored root-relative.
const MAX_PLY: Score = 256;

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Flag {
    /// `alpha < score < beta`: the value is exact.
    Exact = 0,
    /// `score >= beta`: a fail-high lower bound.
    Lower = 1,
    /// `score <= alpha`: a fail-low upper bound.
    Upper = 2,
}

impl Flag {
    const fn from_u8(bits: u8) -> Self {
        match bits {
            0 => Flag::Exact,
            1 => Flag::Lower,
            _ => Flag::Upper,
        }
    }
}

/// A single 64-bit slot, packed as:
/// - bits 0..16:  key (high 16 bits of the zobrist hash)
/// - bits 16..32: best move (`Move` is internally `u16`)
/// - bits 32..48: score (`i16`)
/// - bits 48..56: depth (`u8`)
/// - bits 56..64: flag (`u8`)
///
/// One word means every read and write is a single non-tearing atomic
/// operation, so probes and stores can be shared across search threads
/// without locks.
#[derive(Clone, Copy)]
pub struct Entry(u64);

impl Entry {
    const EMPTY: Entry = Entry(0);

    fn pack(key: u16, best: Move, score: i16, depth: u8, flag: u8) -> Self {
        let bits = (key as u64)
            | ((best.to_bits() as u64) << 16)
            | ((score as u16 as u64) << 32)
            | ((depth as u64) << 48)
            | ((flag as u64) << 56);
        Entry(bits)
    }

    const fn key(self) -> u16 {
        (self.0 & 0xFFFF) as u16
    }

    pub const fn best(self) -> Move {
        Move::from_bits(((self.0 >> 16) & 0xFFFF) as u16)
    }

    /// `ply` is the distance from root at the probe site, used to shift
    /// stored mate scores back into ply-relative form.
    pub const fn score(self, ply: u32) -> Score {
        let s = ((self.0 >> 32) & 0xFFFF) as i16 as Score;
        if s > MATE - MAX_PLY {
            s - ply as Score
        } else if s < -MATE + MAX_PLY {
            s + ply as Score
        } else {
            s
        }
    }

    pub const fn depth(self) -> u32 {
        ((self.0 >> 48) & 0xFF) as u8 as u32
    }

    pub const fn flag(self) -> Flag {
        Flag::from_u8(((self.0 >> 56) & 0xFF) as u8)
    }
}

pub struct TranspositionTable {
    entries: Vec<AtomicU64>,
    mask: usize,
}

impl TranspositionTable {
    pub fn new(size_mb: usize) -> Self {
        let bytes = size_mb * 1024 * 1024;
        let entry_size = std::mem::size_of::<AtomicU64>();
        let desired = bytes / entry_size;
        let n = if desired == 0 {
            1
        } else {
            1 << (usize::BITS - desired.leading_zeros() - 1)
        };
        let entries: Vec<AtomicU64> = (0..n).map(|_| AtomicU64::new(0)).collect();
        Self {
            entries,
            mask: n - 1,
        }
    }

    pub fn clear(&self) {
        for slot in &self.entries {
            slot.store(0, Ordering::Relaxed);
        }
    }

    pub fn probe(&self, hash: u64) -> Option<Entry> {
        let index = (hash as usize) & self.mask;
        let entry = Entry(self.entries[index].load(Ordering::Relaxed));
        if entry.key() == (hash >> 48) as u16 {
            Some(entry)
        } else {
            None
        }
    }

    pub fn store(
        &self,
        hash: u64,
        best: Option<Move>,
        score: Score,
        depth: u32,
        ply: u32,
        flag: Flag,
    ) {
        let index = (hash as usize) & self.mask;
        let key = (hash >> 48) as u16;
        let stored_score = if score > MATE - MAX_PLY {
            score + ply as Score
        } else if score < -MATE + MAX_PLY {
            score - ply as Score
        } else {
            score
        };
        let new_entry = Entry::pack(
            key,
            best.unwrap_or(Move::NULL),
            stored_score as i16,
            depth as u8,
            flag as u8,
        );
        let new_bits = new_entry.0;
        loop {
            let old_bits = self.entries[index].load(Ordering::Relaxed);
            let old = Entry(old_bits);
            // Depth-preferred: never evict a deeper entry for a different
            // position. Same position always updates.
            if old.key() != key && depth < old.depth() {
                return;
            }
            match self.entries[index].compare_exchange(
                old_bits,
                new_bits,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return,
                Err(_) => continue,
            }
        }
    }
}
