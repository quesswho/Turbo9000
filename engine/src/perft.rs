use crate::movegen::{generate_all, MoveList};
use crate::position::{Black, Position, Side, White};

/// Counts the leaves of the move tree.
pub fn perft(position: &mut Position, depth: u32) -> u64 {
    let mut lists = vec![MoveList::new(); depth as usize];
    if position.side_to_move().is_white() {
        run::<White>(position, depth, &mut lists)
    } else {
        run::<Black>(position, depth, &mut lists)
    }
}

/// The recursion flips the side as a type, so the color is only a runtime value
/// at the root. Each ply keeps its own list, since building one costs more than
/// the moves that go in it.
fn run<Us: Side>(position: &mut Position, depth: u32, lists: &mut [MoveList]) -> u64 {
    if depth == 0 {
        return 1;
    }

    let (list, deeper) = lists.split_first_mut().unwrap();
    generate_all::<Us>(position, list);

    if depth == 1 {
        return list.len() as u64;
    }

    let mut nodes = 0;
    for &mv in list.moves() {
        let undo = position.make_move(mv);
        nodes += run::<Us::Them>(position, depth - 1, deeper);
        position.unmake_move(mv, undo);
    }
    nodes
}
