use std::io::Write;
use std::process::{Command, Stdio};

/// Feeds a whole session to the engine and hands back what it wrote.
fn session(input: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_turbo9000"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("engine did not start");
    child
        .stdin
        .take()
        .expect("no stdin")
        .write_all(input.as_bytes())
        .expect("engine closed stdin");
    let output = child.wait_with_output().expect("engine did not exit");
    String::from_utf8(output.stdout).expect("engine wrote invalid utf8")
}

#[test]
fn handshake() {
    let output = session("uci\nisready\nquit\n");
    assert!(output.contains("id name Turbo9000"), "{output}");
    assert!(output.contains("uciok"), "{output}");
    assert!(output.contains("readyok"), "{output}");
}

#[test]
fn moves_are_played_out() {
    let output = session("position startpos moves e2e4 e7e5 g1f3\ngo perft 1\nquit\n");
    assert!(output.contains("Nodes searched: 29"), "{output}");
}

#[test]
fn promotions_are_played_out() {
    let output = session("position fen 8/P6k/8/8/8/8/7K/8 w - - 0 1 moves a7a8n\ngo perft 1\nquit\n");
    assert!(output.contains("Nodes searched: 5"), "{output}");
}

#[test]
fn a_bad_move_leaves_the_position_alone() {
    let output = session("position startpos moves e2e5\ngo perft 1\nquit\n");
    assert!(output.contains("Nodes searched: 20"), "{output}");
}

#[test]
fn a_search_answers_with_a_move() {
    let output = session("position startpos\ngo depth 2\nquit\n");
    assert!(output.contains("bestmove "), "{output}");
}

#[test]
fn a_forced_mate_is_found() {
    let output = session("position fen 6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1\ngo depth 2\nquit\n");
    assert!(output.contains("bestmove a1a8"), "{output}");
}

#[test]
fn a_search_reports_the_nodes_it_spent() {
    let output = session("position startpos\ngo depth 2\nquit\n");
    assert!(output.contains(" nodes "), "{output}");
    assert!(output.contains("info depth 2 multipv 1 score cp "), "{output}");
}

#[test]
fn a_clock_bounded_search_answers_with_a_move() {
    let output = session("position startpos\ngo wtime 1000 btime 1000 winc 0 binc 0\nquit\n");
    assert!(output.contains("bestmove "), "{output}");
}

#[test]
fn the_side_to_move_reads_its_own_clock() {
    let output = session("position startpos moves e2e4\ngo wtime 60000 btime 400 winc 0 binc 0\nquit\n");
    let reported: u64 = output
        .split_whitespace()
        .skip_while(|token| *token != "time")
        .nth(1)
        .expect("no time reported")
        .parse()
        .expect("time was not a number");
    assert!(reported < 500, "{output}");
}
