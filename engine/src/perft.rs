use crate::movegen::{check_masks, generate, MoveList};
use crate::position::{Black, Position, Side, White};

/// Counts the leaves of the move tree.
pub fn perft(position: &mut Position, depth: u32) -> u64 {
    if position.side_to_move().is_white() {
        run::<White>(position, depth)
    } else {
        run::<Black>(position, depth)
    }
}

/// The recursion flips the side as a type, so the color is only a runtime value
/// at the root.
fn run<Us: Side>(position: &mut Position, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }

    let mut list = MoveList::new();
    let masks = check_masks::<Us>(position);
    if masks.in_check() {
        generate::<Us, true, true>(position, &masks, &mut list);
    } else {
        generate::<Us, true, false>(position, &masks, &mut list);
        generate::<Us, false, true>(position, &masks, &mut list);
    }

    if depth == 1 {
        return list.len() as u64;
    }

    let mut nodes = 0;
    for &mv in list.moves() {
        let undo = position.make_move(mv);
        nodes += run::<Us::Them>(position, depth - 1);
        position.unmake_move(mv, undo);
    }
    nodes
}
