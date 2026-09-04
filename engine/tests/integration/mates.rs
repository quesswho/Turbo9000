use crate::common::{check, score_of};
use engine::search::MATE;

#[test]
fn back_rank() {
    check("6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1", 1, "a1a8");
}

/// `f6g6` and `f6f7` both mate in two and the root shuffle picks between them,
/// so the distance to mate is asserted instead of the move.
#[test]
fn rook_and_king() {
    assert_eq!(score_of("7k/8/5K2/8/8/8/8/R7 w - - 0 1", 4), MATE - 3);
}

/// Every black reply to `a1a6` loses, so a pass is worth more than any move
/// here. Null move pruning only stops believing that once the verification
/// search reaches the mate, which takes eight plies rather than four.
#[test]
fn rook_sacrifice() {
    check("kbK5/pp6/1P6/8/8/8/8/R6R w - - 0 1", 4, "a1a6");
}