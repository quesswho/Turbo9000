use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::nnue::evaluate;
use crate::movegen::{check_masks, generate, generate_all, MoveList, HISTORY_MAX};
use crate::moves::Move;
use crate::position::{Black, Color, Position, Side, White};
use crate::ttable::{Flag, TranspositionTable};

pub type Score = i32;

pub const MATE: Score = 30_000;

/// How deep a time limited search is allowed to iterate.
const MAX_DEPTH: u32 = 64;

/// Number of nodes per budget check
const CHECK_INTERVAL: u64 = 2048;

/// How many plies of captures the quiescence search may follow.
const QUIESCENCE_DEPTH: usize = 8;

/// Ranks every quiet move at zero, for the nodes that order without history.
const EMPTY_HISTORY: [[i32; 64]; 64] = [[0; 64]; 64];

/// Half width of the window opened around the previous iteration's score.
const ASPIRATION_DELTA: Score = 25;

/// Below this depth the previous score is too rough to narrow the window with.
const ASPIRATION_MIN_DEPTH: u32 = 6;

const LMR_MIN_DEPTH: u32 = 3;
const LMR_MIN_MOVES: usize = 3;

/// How much to shave off a late quiet move.
fn reduction(depth: u32, index: usize) -> u32 {
    let late = (index - LMR_MIN_MOVES) as u32;
    (1 + (depth - LMR_MIN_DEPTH) / 6 + late / 8).min(depth - 2)
}

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
struct Searcher<'a> {
    table: &'a TranspositionTable,
    lists: Vec<MoveList>,
    killers: Vec<[Move; 2]>,
    history: Vec<[[i32; 64]; 64]>,
    seen: Vec<u64>,
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

impl Searcher<'_> {
    /// A quiet move that caused a cutoff is tried first at the same ply of a
    /// sibling line, where it very often cuts again.
    fn remember_killer(&mut self, mv: Move, ply: usize) {
        if self.killers[ply][0] != mv {
            self.killers[ply][1] = self.killers[ply][0];
            self.killers[ply][0] = mv;
        }
    }

    /// Quiet moves that cut anywhere in the tree are tried before the rest.
    /// The bonus decays towards `HISTORY_MAX` so the table cannot run away.
    fn reward_history(&mut self, mv: Move, side: usize, depth: u32) {
        let bonus = ((depth * depth) as i32).min(HISTORY_MAX);
        let entry = &mut self.history[side][mv.from() as usize][mv.to() as usize];
        *entry += bonus - *entry * bonus / HISTORY_MAX;
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
        killers: vec![[Move::NULL; 2]; limits.depth as usize + QUIESCENCE_DEPTH],
        history: vec![[[0; 64]; 64]; Color::COUNT],
        seen,
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
            searcher.aspiration::<White>(position, iteration, score)
        } else {
            searcher.aspiration::<Black>(position, iteration, score)
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
    /// Widens the side the score escaped from until it lands inside the window.
    fn aspiration<Us: Side>(
        &mut self,
        position: &mut Position,
        depth: u32,
        previous: Score,
    ) -> (Option<Move>, Score) {
        if depth < ASPIRATION_MIN_DEPTH {
            return self.root::<Us>(position, depth, -MATE, MATE);
        }

        let mut delta = ASPIRATION_DELTA;
        let mut alpha = (previous - delta).max(-MATE);
        let mut beta = (previous + delta).min(MATE);
        loop {
            let (best, score) = self.root::<Us>(position, depth, alpha, beta);
            if self.stopped {
                return (best, score);
            }
            // A bound already at the mate score cannot widen.
            if score <= alpha && alpha > -MATE {
                alpha = (score - delta).max(-MATE);
            } else if score >= beta && beta < MATE {
                beta = (score + delta).min(MATE);
            } else {
                return (best, score);
            }
            delta *= 2;
        }
    }

    fn root<Us: Side>(
        &mut self,
        position: &mut Position,
        depth: u32,
        mut alpha: Score,
        beta: Score,
    ) -> (Option<Move>, Score) {
        self.visit();
        let hash = position.hash();
        let hint = self.table.probe(hash).map_or(Move::NULL, |e| e.best());
        generate_all::<Us>(position, &mut self.lists[0]);
        let killers = self.killers[0];
        let side = position.side_to_move().index();
        self.lists[0].score(position, hint, killers, &self.history[side]);

        let mut best = None;

        self.seen.push(hash);
        for index in 0..self.lists[0].len() {
            let mv = self.lists[0].pick(index);
            let undo = position.make_move(mv);
            let mut score = if index == 0 {
                -self.negamax::<Us::Them>(position, depth - 1, 1, -beta, -alpha)
            } else {
                -self.negamax::<Us::Them>(position, depth - 1, 1, -alpha - 1, -alpha)
            };
            if index > 0 && score > alpha && score < beta {
                score = -self.negamax::<Us::Them>(position, depth - 1, 1, -beta, -alpha);
            }
            position.unmake_move(mv, undo);
            if score > alpha {
                (best, alpha) = (Some(mv), score);
                if alpha >= beta {
                    break;
                }
            }
            if self.stopped {
                break;
            }
        }
        self.seen.pop();

        // A fail low leaves `best` unset, keeping the previous iteration's hint.
        if !self.stopped && best.is_some() {
            let flag = if alpha >= beta { Flag::Lower } else { Flag::Exact };
            self.table.store(hash, best, alpha, depth, 0, flag);
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
        let killers = self.killers[here];
        let side = position.side_to_move().index();
        self.lists[here].score(position, tt_move, killers, &self.history[side]);

        let alpha_orig = alpha;
        let mut best_move = None;
        // The list is not empty, so some move always beats this.
        let mut best_score = -MATE;

        self.seen.push(hash);
        for index in 0..self.lists[here].len() {
            let mv = self.lists[here].pick(index);
            let undo = position.make_move(mv);
            // Ordering makes the first move the likely best, so it is worth a
            // full window.
            let mut score = if index == 0 {
                -self.negamax::<Us::Them>(position, depth - 1, ply + 1, -beta, -alpha)
            } else {
                let cut = if depth >= LMR_MIN_DEPTH
                    && index >= LMR_MIN_MOVES
                    && !in_check
                    && !mv.is_capture()
                    && !mv.is_promotion()
                {
                    reduction(depth, index)
                } else {
                    0
                };
                let shallow = -self.negamax::<Us::Them>(
                    position,
                    depth - 1 - cut,
                    ply + 1,
                    -alpha - 1,
                    -alpha,
                );
                if cut > 0 && shallow > alpha {
                    -self.negamax::<Us::Them>(position, depth - 1, ply + 1, -alpha - 1, -alpha)
                } else {
                    shallow
                }
            };
            if index > 0 && score > alpha && score < beta {
                score = -self.negamax::<Us::Them>(position, depth - 1, ply + 1, -beta, -alpha);
            }
            position.unmake_move(mv, undo);
            if score > best_score {
                best_score = score;
                if score > alpha {
                    alpha = score;
                    best_move = Some(mv);
                }
                if alpha >= beta {
                    self.seen.pop();
                    if !mv.is_capture() && !mv.is_promotion() {
                        self.remember_killer(mv, here);
                        self.reward_history(mv, position.side_to_move().index(), depth);
                    }
                    if !self.stopped {
                        self.table.store(hash, Some(mv), best_score, depth, ply, Flag::Lower);
                    }
                    return best_score;
                }
            }
        }
        self.seen.pop();

        if !self.stopped {
            let flag = if alpha > alpha_orig { Flag::Exact } else { Flag::Upper };
            self.table.store(hash, best_move, best_score, depth, ply, flag);
        }
        best_score
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
            return stand_pat;
        }
        if stand_pat > alpha {
            alpha = stand_pat;
        }

        // Standing pat is a floor: the side to move need not enter a capture.
        let mut best_score = stand_pat;
        let here = ply as usize;
        if here >= self.lists.len() || self.stopped {
            return best_score;
        }

        let masks = check_masks::<Us>(position);
        self.lists[here].clear();
        generate::<Us, true, false>(position, &masks, &mut self.lists[here]);
        self.lists[here].score(position, Move::NULL, [Move::NULL; 2], &EMPTY_HISTORY);

        for index in 0..self.lists[here].len() {
            let mv = self.lists[here].pick(index);
            let undo = position.make_move(mv);
            let score = -self.quiescence::<Us::Them>(position, -beta, -alpha, ply + 1);
            position.unmake_move(mv, undo);
            if score > best_score {
                best_score = score;
                if score > alpha {
                    alpha = score;
                }
                if alpha >= beta {
                    return best_score;
                }
            }
        }
        best_score
    }
}
