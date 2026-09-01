use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::nnue::evaluate;
use crate::movegen::{check_masks, generate, generate_all, MoveList};
use crate::moves::Move;
use crate::position::{Black, Position, Side, White};
use crate::ttable::{Flag, TranspositionTable};
use crate::zobrist::splitmix64;

pub type Score = i32;

pub const MATE: Score = 30_000;

/// How deep a time limited search is allowed to iterate.
const MAX_DEPTH: u32 = 64;

/// Number of nodes per budget check
const CHECK_INTERVAL: u64 = 2048;

/// How many plies of captures the quiescence search may follow.
const QUIESCENCE_DEPTH: usize = 8;

#[derive(Clone)]
pub struct Limits {
    depth: u32,
    deadline: Option<Instant>,
    stop: Option<Arc<AtomicBool>>,
}

impl Limits {
    pub const fn depth(depth: u32) -> Self {
        Self {
            depth,
            deadline: None,
            stop: None,
        }
    }

    /// Start the clock
    pub fn time(span: Duration) -> Self {
        Self {
            depth: MAX_DEPTH,
            deadline: Some(Instant::now() + span),
            stop: None,
        }
    }

    pub const fn infinite() -> Self {
        Self {
            depth: MAX_DEPTH,
            deadline: None,
            stop: None,
        }
    }

    /// Cut the search short once `flag` is raised.
    pub fn stopped_by(mut self, flag: Arc<AtomicBool>) -> Self {
        self.stop = Some(flag);
        self
    }
}

/// Successive searches draw different seeds, so a position that comes round
/// again is not answered with the same move.
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Rng(u64);

impl Rng {
    fn below(&mut self, bound: usize) -> usize {
        (splitmix64(&mut self.0) % bound as u64) as usize
    }
}

/// Per thread budget
struct Searcher<'a> {
    table: &'a TranspositionTable,
    lists: Vec<MoveList>,
    seen: Vec<u64>,
    rng: Rng,
    nodes: u64,
    deadline: Option<Instant>,
    stop: Option<Arc<AtomicBool>>,
    stopped: bool,
}

impl Searcher<'_> {
    fn visit(&mut self) {
        self.nodes += 1;
        if self.nodes & (CHECK_INTERVAL - 1) == 0 {
            if let Some(deadline) = self.deadline {
                self.stopped |= Instant::now() >= deadline;
            }
            if let Some(flag) = &self.stop {
                self.stopped |= flag.load(Ordering::Relaxed);
            }
        }
    }
}

/// Results returned from a search
pub struct Report {
    pub best: Option<Move>,
    pub score: Score,
    pub depth: u32,
    pub nodes: u64,
}

fn repeats(seen: &[u64], hash: u64, halfmove_clock: u8) -> bool {
    let span = (halfmove_clock as usize).min(seen.len());
    let window = &seen[seen.len() - span..];
    let mut index = window.len();
    while index >= 2 {
        index -= 2;
        if window[index] == hash {
            return true;
        }
    }
    false
}

pub fn search(
    position: &mut Position,
    limits: Limits,
    table: &TranspositionTable,
    history: &[u64],
) -> Report {
    debug_assert!(limits.depth >= 1, "a search shallower than one ply has no move");
    let mut seen = Vec::with_capacity(history.len() + limits.depth as usize + 1);
    seen.extend_from_slice(history);
    let mut searcher = Searcher {
        table,
        lists: vec![MoveList::new(); limits.depth as usize + QUIESCENCE_DEPTH],
        seen,
        rng: Rng(position.hash() ^ SEQUENCE.fetch_add(1, Ordering::Relaxed)),
        nodes: 0,
        deadline: limits.deadline,
        stop: limits.stop,
        stopped: false,
    };

    let mut best = None;
    let mut score = 0;
    let mut completed = 0;
    for iteration in 1..=limits.depth {
        let (found, found_score) = if position.side_to_move().is_white() {
            searcher.root::<White>(position, iteration)
        } else {
            searcher.root::<Black>(position, iteration)
        };

        if searcher.stopped {
            if best.is_none() {
                (best, score) = (found, found_score);
            }
            break;
        }
        (best, score, completed) = (found, found_score, iteration);
    }
    Report {
        best,
        score,
        depth: completed,
        nodes: searcher.nodes,
    }
}

impl Searcher<'_> {
    fn root<Us: Side>(
        &mut self,
        position: &mut Position,
        depth: u32,
    ) -> (Option<Move>, Score) {
        self.visit();
        let hash = position.hash();
        generate_all::<Us>(position, &mut self.lists[0]);
        // Material alone ties every quiet move, and `pick` keeps the first of
        // an equal ranked run, so a shuffle here is what stops the search
        // answering a repeated position with the move that repeats it.
        for index in (1..self.lists[0].len()).rev() {
            let other = self.rng.below(index + 1);
            self.lists[0].moves_mut().swap(index, other);
        }
        // No transposition hint at the root. It would outrank the shuffle and
        // answer a position the game has already visited with the same move.
        self.lists[0].score(position, Move::NULL);

        let mut best = None;
        let mut alpha = -MATE;

        self.seen.push(hash);
        for index in 0..self.lists[0].len() {
            let mv = self.lists[0].pick(index);
            let undo = position.make_move(mv);
            let score = -self.negamax::<Us::Them>(position, depth - 1, 1, -MATE, -alpha);
            position.unmake_move(mv, undo);
            if score > alpha {
                (best, alpha) = (Some(mv), score);
            }
            if self.stopped {
                break;
            }
        }
        self.seen.pop();

        if !self.stopped && best.is_some() {
            self.table.store(hash, best, alpha, depth, 0, Flag::Exact);
        }
        (best, alpha)
    }

    /// alpha-beta search
    fn negamax<Us: Side>(
        &mut self,
        position: &mut Position,
        depth: u32,
        ply: u32,
        mut alpha: Score,
        beta: Score,
    ) -> Score {
        self.visit();
        if depth == 0 {
            return self.quiescence::<Us>(position, alpha, beta, ply);
        }

        // A draw at the horizon is missed, the node above it catches the line.
        let halfmove_clock = position.halfmove_clock();
        let hash = position.hash();
        if halfmove_clock >= 100
            || halfmove_clock >= 4 && repeats(&self.seen, hash, halfmove_clock)
        {
            return 0;
        }

        if self.stopped {
            return 0;
        }

        let tt_entry = self.table.probe(hash);
        if let Some(entry) = tt_entry {
            if entry.depth() >= depth {
                let score = entry.score(ply);
                match entry.flag() {
                    Flag::Exact => return score,
                    Flag::Lower if score >= beta => return score,
                    Flag::Upper if score <= alpha => return score,
                    _ => {}
                }
            }
        }

        // Every ply keeps its own movelist
        let here = ply as usize;
        let in_check = generate_all::<Us>(position, &mut self.lists[here]);
        if self.lists[here].is_empty() {
            return if in_check { ply as Score - MATE } else { 0 };
        }

        let tt_move = tt_entry.map_or(Move::NULL, |e| e.best());
        self.lists[here].score(position, tt_move);

        let alpha_orig = alpha;
        let mut best_move = None;

        self.seen.push(hash);
        for index in 0..self.lists[here].len() {
            let mv = self.lists[here].pick(index);
            let undo = position.make_move(mv);
            let score = -self.negamax::<Us::Them>(position, depth - 1, ply + 1, -beta, -alpha);
            position.unmake_move(mv, undo);
            if score >= beta {
                self.seen.pop();
                if !self.stopped {
                    self.table.store(hash, Some(mv), score, depth, ply, Flag::Lower);
                }
                return beta;
            }
            if score > alpha {
                alpha = score;
                best_move = Some(mv);
            }
        }
        self.seen.pop();

        if !self.stopped {
            let flag = if alpha > alpha_orig { Flag::Exact } else { Flag::Upper };
            self.table.store(hash, best_move, alpha, depth, ply, flag);
        }
        alpha
    }

    /// Plays out the captures so the leaf is not evaluated mid trade.
    fn quiescence<Us: Side>(
        &mut self,
        position: &mut Position,
        mut alpha: Score,
        beta: Score,
        ply: u32,
    ) -> Score {
        self.visit();
        let stand_pat = evaluate(position);
        if stand_pat >= beta {
            return beta;
        }
        if stand_pat > alpha {
            alpha = stand_pat;
        }

        let here = ply as usize;
        if here >= self.lists.len() || self.stopped {
            return alpha;
        }

        let masks = check_masks::<Us>(position);
        self.lists[here].clear();
        generate::<Us, true, false>(position, &masks, &mut self.lists[here]);
        self.lists[here].score(position, Move::NULL);

        for index in 0..self.lists[here].len() {
            let mv = self.lists[here].pick(index);
            let undo = position.make_move(mv);
            let score = -self.quiescence::<Us::Them>(position, -beta, -alpha, ply + 1);
            position.unmake_move(mv, undo);
            if score >= beta {
                return beta;
            }
            if score > alpha {
                alpha = score;
            }
        }
        alpha
    }
}
