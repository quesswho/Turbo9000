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
