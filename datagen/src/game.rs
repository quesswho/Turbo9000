use engine::movegen::{generate_all, MoveList};
use engine::position::{Black, Piece, Position, White};
use engine::search::{search, Limits, Score, MATE};
use engine::ttable::TranspositionTable;

use crate::pack::{pack, set_outcome, Outcome, RECORD};
use crate::rng::Rng;

/// Random plies played before the search takes over.
const OPENING_PLIES: usize = 8;

/// A random opening this lopsided has already decided the game.
const OPENING_BALANCE: Score = 400;

/// Scores this close to `MATE` are a distance to mate, not an evaluation.
const MAX_MATE_PLIES: Score = 128;

fn legal(position: &Position, list: &mut MoveList) -> bool {
    if position.side_to_move().is_white() {
        generate_all::<White>(position, list)
    } else {
        generate_all::<Black>(position, list)
    }
}

/// The third visit to a position draws, so two earlier ones are enough.
fn repeated(history: &[u64], hash: u64, halfmove_clock: u8) -> bool {
    let span = (halfmove_clock as usize).min(history.len());
    history[history.len() - span..]
        .iter()
        .filter(|&&seen| seen == hash)
        .count()
        >= 2
}

fn insufficient(position: &Position) -> bool {
    let mating = position.pieces_of_kind(Piece::Pawn)
        | position.pieces_of_kind(Piece::Rook)
        | position.pieces_of_kind(Piece::Queen);
    let minors = position.pieces_of_kind(Piece::Knight) | position.pieces_of_kind(Piece::Bishop);
    mating == 0 && minors.count_ones() <= 1
}

fn opening(rng: &mut Rng, list: &mut MoveList) -> Option<(Position, Vec<u64>)> {
    let mut position = Position::starting();
    let mut history = Vec::new();
    for _ in 0..=OPENING_PLIES {
        legal(&position, list);
        if list.is_empty() {
            return None;
        }
        if history.len() == OPENING_PLIES {
            return Some((position, history));
        }
        let mv = list.moves()[rng.below(list.len())];
        history.push(position.hash());
        position.make_move(mv);
    }
    None
}

/// Plays one game and appends a record for every quiet position in it.
pub fn play(
    rng: &mut Rng,
    table: &TranspositionTable,
    depth: u32,
    records: &mut Vec<[u8; RECORD]>,
) {
    table.clear();
    let mut list = MoveList::new();
    let Some((mut position, mut history)) = opening(rng, &mut list) else {
        return;
    };

    let opened = search(&mut position, Limits::depth(depth), table, &history);
    if opened.score.abs() > OPENING_BALANCE {
        return;
    }

    let start = records.len();
    let mut movers = Vec::new();
    let outcome = loop {
        let in_check = legal(&position, &mut list);
        if list.is_empty() {
            break match (in_check, position.side_to_move().is_white()) {
                (false, _) => Outcome::Draw,
                (true, true) => Outcome::BlackWin,
                (true, false) => Outcome::WhiteWin,
            };
        }
        if position.halfmove_clock() >= 100
            || repeated(&history, position.hash(), position.halfmove_clock())
            || insufficient(&position)
        {
            break Outcome::Draw;
        }

        let report = search(&mut position, Limits::depth(depth), table, &history);
        let Some(mv) = report.best else {
            break Outcome::Draw;
        };

        let quiet = !in_check && !mv.is_capture() && !mv.is_promotion();
        if quiet && report.score.abs() < MATE - MAX_MATE_PLIES {
            records.push(pack(&position, report.score));
            movers.push(position.side_to_move().is_white());
        }

        history.push(position.hash());
        position.make_move(mv);
    };

    for (record, &white) in records[start..].iter_mut().zip(&movers) {
        set_outcome(record, outcome, white);
    }
}
