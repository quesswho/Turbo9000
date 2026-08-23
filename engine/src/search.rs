use std::time::{Duration, Instant};

use crate::movegen::{generate_all, MoveList};
use crate::moves::Move;
use crate::position::{Black, Piece, Position, Side, White};

pub type Score = i32;

pub const MATE: Score = 30_000;

/// How deep a time limited search is allowed to iterate.
const MAX_DEPTH: u32 = 64;

/// Number of nodes per budget check
const CHECK_INTERVAL: u64 = 2048;

#[derive(Clone, Copy)]
pub struct Limits {
    depth: u32,
    deadline: Option<Instant>,
}

impl Limits {
    pub const fn depth(depth: u32) -> Self {
        Self {
            depth,
            deadline: None,
        }
    }

    /// Start the clock
    pub fn time(span: Duration) -> Self {
        Self {
            depth: MAX_DEPTH,
            deadline: Some(Instant::now() + span),
        }
    }
}

/// Per thread budget
struct Budget {
    nodes: u64,
    deadline: Option<Instant>,
    stopped: bool,
}

impl Budget {
    fn visit(&mut self) {
        self.nodes += 1;
        if self.nodes & (CHECK_INTERVAL - 1) == 0 {
            if let Some(deadline) = self.deadline {
                self.stopped |= Instant::now() >= deadline;
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
    pub depth: u32,
    pub nodes: u64,
}

pub fn search(position: &mut Position, limits: Limits) -> Report {
    debug_assert!(limits.depth >= 1, "a search shallower than one ply has no move");
    let mut lists = vec![MoveList::new(); limits.depth as usize];
    let mut budget = Budget {
        nodes: 0,
        deadline: limits.deadline,
        stopped: false,
    };

    let mut best = None;
    let mut completed = 0;
    for iteration in 1..=limits.depth {
        let found = if position.side_to_move().is_white() {
            root::<White>(position, iteration, &mut lists, &mut budget)
        } else {
            root::<Black>(position, iteration, &mut lists, &mut budget)
        };

        if budget.stopped {
            best = best.or(found);
            break;
        }
        (best, completed) = (found, iteration);
    }
    Report {
        best,
        depth: completed,
        nodes: budget.nodes,
    }
}

fn root<Us: Side>(
    position: &mut Position,
    depth: u32,
    lists: &mut [MoveList],
    budget: &mut Budget,
) -> Option<Move> {
    budget.visit();
    let (list, deeper) = lists.split_first_mut().unwrap();
    generate_all::<Us>(position, list);

    let mut best = None;
    let mut best_score = Score::MIN;
    for &mv in list.moves() {
        let undo = position.make_move(mv);
        let score = -negamax::<Us::Them>(position, depth - 1, 1, deeper, budget);
        position.unmake_move(mv, undo);
        if score > best_score {
            (best, best_score) = (Some(mv), score);
        }
        if budget.stopped {
            break;
        }
    }
    best
}

fn negamax<Us: Side>(
    position: &mut Position,
    depth: u32,
    ply: u32,
    lists: &mut [MoveList],
    budget: &mut Budget,
) -> Score {
    budget.visit();
    if depth == 0 {
        return evaluate(position);
    }
    
    if budget.stopped {
        return 0;
    }

    // Every ply keeps its own movelist
    let (list, deeper) = lists.split_first_mut().unwrap();
    let in_check = generate_all::<Us>(position, list);
    if list.is_empty() {
        return if in_check { ply as Score - MATE } else { 0 };
    }

    let mut best = Score::MIN;
    for &mv in list.moves() {
        let undo = position.make_move(mv);
        let score = -negamax::<Us::Them>(position, depth - 1, ply + 1, deeper, budget);
        position.unmake_move(mv, undo);
        best = best.max(score);
    }
    best
}
