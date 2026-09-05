use engine::movegen::{generate_all, MoveList};
use engine::position::{BitBoard, Black, Color, Piece, Position, White};
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

/// Dark squares, `a1` among them. Two bishops can only force mate if one sits
/// on each half of this mask.
const DARK: BitBoard = 0xAA55_AA55_AA55_AA55;

/// A lone king loses to a queen, a rook, two opposite colored bishops, a
/// bishop with a knight, or three knights. Two knights, bishops of one
/// color, a single minor and pawns alone cannot force a mate.
fn forces_mate(position: &Position, us: Color) -> bool {
    if position.queens(us).count_ones() > 0 || position.rooks(us).count_ones() > 0 {
        return true;
    }
    let knights = position.knights(us).count_ones();
    let bishops = position.bishops(us);
    knights >= 3
        || (knights >= 1 && bishops.count_ones() >= 1)
        || (bishops.count_ones() >= 2 && bishops & DARK != 0 && bishops & !DARK != 0)
}

fn bare_king(position: &Position, color: Color) -> bool {
    position.color(color) == position.king(color)
}

/// A side with a bare king that faces mating material has the game decided by
/// the material alone, so those positions are stamped with the result when
/// they are recorded rather than at the end of the game.
fn adjudicable(position: &Position) -> Option<Outcome> {
    for color in Color::ALL {
        if bare_king(position, color) && forces_mate(position, color.flip()) {
            return Some(if color.is_white() {
                Outcome::BlackWin
            } else {
                Outcome::WhiteWin
            });
        }
    }
    None
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
    nodes: u64,
    records: &mut Vec<[u8; RECORD]>,
) {
    table.clear();
    let mut list = MoveList::new();
    let Some((mut position, mut history)) = opening(rng, &mut list) else {
        return;
    };

    let opened = search(&mut position, Limits::nodes(nodes), table, &history);
    if opened.score.abs() > OPENING_BALANCE {
        return;
    }

    let start = records.len();
    let mut movers = Vec::new();
    let mut adjudicated = Vec::new();
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

        let report = search(&mut position, Limits::nodes(nodes), table, &history);
        let Some(mv) = report.best else {
            break Outcome::Draw;
        };

        let quiet = !in_check && !mv.is_capture() && !mv.is_promotion();
        if quiet && report.score.abs() < MATE - MAX_MATE_PLIES {
            records.push(pack(&position, report.score));
            movers.push(position.side_to_move().is_white());
            adjudicated.push(adjudicable(&position));
        }

        history.push(position.hash());
        position.make_move(mv);
    };

    for (record, (&white, &decided)) in records[start..]
        .iter_mut()
        .zip(movers.iter().zip(&adjudicated))
    {
        set_outcome(record, decided.unwrap_or(outcome), white);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decided(fen: &str) -> Option<Outcome> {
        let position: Position = fen.parse().expect("bad fen");
        adjudicable(&position)
    }

    #[test]
    fn a_lone_king_loses_to_mating_material() {
        assert_eq!(decided("4k3/8/8/8/8/8/8/3R3K w - - 0 1"), Some(Outcome::WhiteWin));
        assert_eq!(decided("4k3/8/8/8/8/8/8/3Q3K w - - 0 1"), Some(Outcome::WhiteWin));
        assert_eq!(decided("4k3/8/8/8/8/8/8/2BB2K1 w - - 0 1"), Some(Outcome::WhiteWin));
        assert_eq!(decided("4k3/8/8/8/8/8/8/3NB2K w - - 0 1"), Some(Outcome::WhiteWin));
        assert_eq!(decided("4k3/8/8/8/8/8/8/3NNNK1 w - - 0 1"), Some(Outcome::WhiteWin));
        assert_eq!(decided("4K3/8/8/8/8/8/8/4r1k1 b - - 0 1"), Some(Outcome::BlackWin));
    }

    #[test]
    fn a_lone_king_holds_what_cannot_force_mate() {
        assert_eq!(decided("4k3/8/8/8/8/8/8/3NN1K1 w - - 0 1"), None);
        assert_eq!(decided("4k3/8/8/8/8/8/8/2B1B1K1 w - - 0 1"), None);
        assert_eq!(decided("4k3/8/8/8/8/8/8/3B2K1 w - - 0 1"), None);
        assert_eq!(decided("4k3/8/8/8/8/8/8/3N2K1 w - - 0 1"), None);
        assert_eq!(decided("4k3/8/8/8/8/8/8/4P2K w - - 0 1"), None);
    }

    #[test]
    fn material_on_both_sides_is_not_adjudicated() {
        assert_eq!(decided("4k3/8/8/8/8/8/8/R3r1K1 w - - 0 1"), None);
        assert_eq!(decided("4k3/8/8/8/8/8/8/R3b1K1 w - - 0 1"), None);
    }
}
