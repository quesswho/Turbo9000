use engine::moves::{Move, MoveFlags};
use engine::search::MATE;
use engine::ttable::{Flag, TranspositionTable};

fn mv() -> Move {
    Move::new(12, 28, MoveFlags::Quiet)
}

#[test]
fn probe_after_store_returns_the_same_entry() {
    let tt = TranspositionTable::new(1);
    let hash = 0x0123_4567_89ab_cdef;

    tt.store(hash, Some(mv()), 100, 5, 3, Flag::Exact);

    let entry = tt.probe(hash).expect("entry missing");
    assert_eq!(entry.best(), mv());
    assert_eq!(entry.score(3), 100);
    assert_eq!(entry.depth(), 5);
    assert_eq!(entry.flag(), Flag::Exact);
}

#[test]
fn probe_returns_none_for_unknown_hash() {
    let tt = TranspositionTable::new(1);
    assert!(tt.probe(0xdead_beef_dead_beef).is_none());
}

#[test]
fn clear_empties_the_table() {
    let tt = TranspositionTable::new(1);
    let hash = 0x0123_4567_89ab_cdef;
    tt.store(hash, Some(mv()), 100, 5, 3, Flag::Exact);
    assert!(tt.probe(hash).is_some());

    tt.clear();
    assert!(tt.probe(hash).is_none());
}

#[test]
fn positive_mate_score_shifts_with_ply() {
    let tt = TranspositionTable::new(1);
    let hash = 0xabcd_0000_0000_0000;

    // Mate in 5 plies from ply 3: score = MATE - 5.
    tt.store(hash, Some(mv()), MATE - 5, 5, 3, Flag::Exact);

    let entry = tt.probe(hash).expect("entry missing");
    // Same ply: round-trips.
    assert_eq!(entry.score(3), MATE - 5);
    // At ply 7 the same mate is 4 plies further away.
    assert_eq!(entry.score(7), MATE - 9);
}

#[test]
fn negative_mate_score_shifts_with_ply() {
    let tt = TranspositionTable::new(1);
    let hash = 0xdcba_0000_0000_0000;

    // Mated in 5 plies from ply 3: score = -(MATE - 5).
    tt.store(hash, Some(mv()), -(MATE - 5), 5, 3, Flag::Exact);

    let entry = tt.probe(hash).expect("entry missing");
    assert_eq!(entry.score(3), -(MATE - 5));
    assert_eq!(entry.score(7), -(MATE - 9));
}

#[test]
fn non_mate_score_is_independent_of_ply() {
    let tt = TranspositionTable::new(1);
    let hash = 0x5555_5555_5555_5555;
    tt.store(hash, Some(mv()), 42, 5, 3, Flag::Exact);

    let entry = tt.probe(hash).expect("entry missing");
    assert_eq!(entry.score(3), 42);
    assert_eq!(entry.score(99), 42);
}

#[test]
fn upper_bound_stores_null_move() {
    let tt = TranspositionTable::new(1);
    let hash = 0xaaaa_bbbb_cccc_dddd;
    tt.store(hash, None, 10, 4, 0, Flag::Upper);

    let entry = tt.probe(hash).expect("entry missing");
    assert_eq!(entry.flag(), Flag::Upper);
    // NULL_MOVE = Move::new(0, 0, Quiet); not a move any legal generator emits.
    assert_eq!(entry.best(), Move::new(0, 0, MoveFlags::Quiet));
}

#[test]
fn depth_preferred_keeps_deeper_entry_for_different_position() {
    // A zero-byte table collapses to a single slot, so every hash collides.
    let tt = TranspositionTable::new(0);
    let deep_hash = 0x1111_2222_3333_4444;
    let shallow_hash = 0x5555_6666_7777_8888;

    tt.store(deep_hash, Some(mv()), 50, 10, 0, Flag::Exact);
    // Shallower store for a different position must not evict the deeper entry.
    tt.store(shallow_hash, Some(mv()), 1, 2, 0, Flag::Exact);

    assert!(tt.probe(deep_hash).is_some());
    assert!(tt.probe(shallow_hash).is_none());
}
