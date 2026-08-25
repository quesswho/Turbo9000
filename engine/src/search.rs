use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::movegen::{check_masks, generate, generate_all, MoveList};
use crate::moves::Move;
use crate::position::{Black, Piece, Position, Side, White};
use crate::ttable::{Flag, TranspositionTable};

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

/// Per thread budget
struct Budget {
    nodes: u64,
    deadline: Option<Instant>,
    stop: Option<Arc<AtomicBool>>,
    stopped: bool,
}

impl Budget {
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

const MATERIAL: [(Piece, Score); 5] = [
    (Piece::Pawn, 100),
    (Piece::Knight, 320),
    (Piece::Bishop, 330),
    (Piece::Rook, 500),
    (Piece::Queen, 900),
];

/// TODO: extremely simplified evaluation for now, move eval to eval.rs 
pub fn evaluate(position: &Position) -> Score {
    let us = position.side_to_move();
    let them = us.flip();
    MATERIAL
        .iter()
        .map(|&(piece, value)| {
            let ours = position.pieces(piece, us).count_ones() as Score;
            let theirs = position.pieces(piece, them).count_ones() as Score;
            value * (ours - theirs)
        })
        .sum()
}

/// Results returned from a search
pub struct Report {
    pub best: Option<Move>,
    pub score: Score,
    pub depth: u32,
    pub nodes: u64,
}

pub fn search(position: &mut Position, limits: Limits, table: &TranspositionTable) -> Report {
    debug_assert!(limits.depth >= 1, "a search shallower than one ply has no move");
    let mut lists = vec![MoveList::new(); limits.depth as usize + QUIESCENCE_DEPTH];
    let mut budget = Budget {
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
            root::<White>(position, iteration, &mut lists, &mut budget, table)
        } else {
            root::<Black>(position, iteration, &mut lists, &mut budget, table)
        };

        if budget.stopped {
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
        nodes: budget.nodes,
    }
}

fn root<Us: Side>(
    position: &mut Position,
    depth: u32,
    lists: &mut [MoveList],
    budget: &mut Budget,
    table: &TranspositionTable,
) -> (Option<Move>, Score) {
    budget.visit();
    let hash = position.hash();
    let hint = table.probe(hash).map(|e| e.best());
    let (list, deeper) = lists.split_first_mut().unwrap();
    generate_all::<Us>(position, list);
    let tt_move = hint.filter(|&mv| list.moves().contains(&mv));

    let mut best = None;
    let mut alpha = -MATE;

    if let Some(mv) = tt_move {
        let undo = position.make_move(mv);
        let score = -negamax::<Us::Them>(position, depth - 1, 1, -MATE, -alpha, deeper, budget, table);
        position.unmake_move(mv, undo);
        if score > alpha {
            (best, alpha) = (Some(mv), score);
        }
        if budget.stopped {
            return (best, alpha);
        }
    }

    for &mv in list.moves() {
        if Some(mv) == tt_move {
            continue;
        }
        let undo = position.make_move(mv);
        let score = -negamax::<Us::Them>(position, depth - 1, 1, -MATE, -alpha, deeper, budget, table);
        position.unmake_move(mv, undo);
        if score > alpha {
            (best, alpha) = (Some(mv), score);
        }
        if budget.stopped {
            break;
        }
    }

    if !budget.stopped && best.is_some() {
        table.store(hash, best, alpha, depth, 0, Flag::Exact);
    }
    (best, alpha)
}

/// alpha-beta search
fn negamax<Us: Side>(
    position: &mut Position,
    depth: u32,
    ply: u32,
    mut alpha: Score,
    beta: Score,
    lists: &mut [MoveList],
    budget: &mut Budget,
    table: &TranspositionTable,
) -> Score {
    budget.visit();
    if depth == 0 {
        return quiescence::<Us>(position, alpha, beta, lists, budget);
    }

    if budget.stopped {
        return 0;
    }

    let hash = position.hash();
    let tt_entry = table.probe(hash);
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
    let (list, deeper) = lists.split_first_mut().unwrap();
    let in_check = generate_all::<Us>(position, list);
    if list.is_empty() {
        return if in_check { ply as Score - MATE } else { 0 };
    }

    let tt_move = tt_entry.and_then(|e| {
        let mv = e.best();
        if list.moves().contains(&mv) { Some(mv) } else { None }
    });

    let alpha_orig = alpha;
    let mut best_move = None;

    if let Some(mv) = tt_move {
        let undo = position.make_move(mv);
        let score = -negamax::<Us::Them>(position, depth - 1, ply + 1, -beta, -alpha, deeper, budget, table);
        position.unmake_move(mv, undo);
        if score >= beta {
            if !budget.stopped {
                table.store(hash, Some(mv), score, depth, ply, Flag::Lower);
            }
            return beta;
        }
        if score > alpha {
            alpha = score;
            best_move = Some(mv);
        }
    }

    for &mv in list.moves() {
        if Some(mv) == tt_move {
            continue;
        }
        let undo = position.make_move(mv);
        let score = -negamax::<Us::Them>(position, depth - 1, ply + 1, -beta, -alpha, deeper, budget, table);
        position.unmake_move(mv, undo);
        if score >= beta {
            if !budget.stopped {
                table.store(hash, Some(mv), score, depth, ply, Flag::Lower);
            }
            return beta;
        }
        if score > alpha {
            alpha = score;
            best_move = Some(mv);
        }
    }

    if !budget.stopped {
        let flag = if alpha > alpha_orig { Flag::Exact } else { Flag::Upper };
        table.store(hash, best_move, alpha, depth, ply, flag);
    }
    alpha
}

/// Plays out the captures so the leaf is not evaluated mid trade.
fn quiescence<Us: Side>(
    position: &mut Position,
    mut alpha: Score,
    beta: Score,
    lists: &mut [MoveList],
    budget: &mut Budget,
) -> Score {
    budget.visit();
    let stand_pat = evaluate(position);
    if stand_pat >= beta {
        return beta;
    }
    if stand_pat > alpha {
        alpha = stand_pat;
    }

    let Some((list, deeper)) = lists.split_first_mut() else {
        return alpha;
    };
    if budget.stopped {
        return alpha;
    }

    let masks = check_masks::<Us>(position);
    list.clear();
    generate::<Us, true, false>(position, &masks, list);

    for &mv in list.moves() {
        let undo = position.make_move(mv);
        let score = -quiescence::<Us::Them>(position, -beta, -alpha, deeper, budget);
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
