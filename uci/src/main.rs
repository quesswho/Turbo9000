use std::io::{self, BufRead};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use engine::movegen::find_move;
use engine::perft::perft;
use engine::position::{Color, Position};
use engine::search::{search, Limits, Score, MATE};
use engine::ttable::TranspositionTable;
use engine::NAME;

/// Slack left on the clock so that reporting a move never flags.
const MOVE_OVERHEAD: u64 = 50;

/// Beyond this distance from mate a score is ordinary centipawns.
const MAX_MATE_PLIES: Score = 128;

fn main() {
    let mut position = Position::starting();
    let mut history = Vec::new();
    let table = Arc::new(TranspositionTable::new(16));
    let mut searching = None;
    let stop = Arc::new(AtomicBool::new(false));

    for line in io::stdin().lock().lines().map_while(Result::ok) {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let Some((command, arguments)) = tokens.split_first() else {
            continue;
        };

        match *command {
            "uci" => {
                println!("id name {NAME}");
                println!("id author adrian-tudev, quesswho");
                println!("uciok");
            }
            "isready" => println!("readyok"),
            "ucinewgame" => {
                wait(&mut searching, &stop);
                position = Position::starting();
                history.clear();
                table.clear();
            }
            "position" => {
                wait(&mut searching, &stop);
                if let Some((parsed, played)) = parse_position(arguments) {
                    (position, history) = (parsed, played);
                }
            }
            "go" => {
                wait(&mut searching, &stop);
                stop.store(false, Ordering::Relaxed);
                match arguments {
                    ["perft", depth] => {
                        if let Ok(depth) = depth.parse() {
                            println!("Nodes searched: {}", perft(&mut position, depth));
                        }
                    }
                    _ => {
                        let limits = parse_limits(arguments, position.side_to_move())
                            .stopped_by(Arc::clone(&stop));
                        let table = Arc::clone(&table);
                        searching = Some(go(position.clone(), limits, table, history.clone()));
                    }
                }
            }
            "stop" => stop.store(true, Ordering::Relaxed),
            "quit" => {
                wait(&mut searching, &stop);
                break;
            }
            _ => {}
        }
    }

    // Input running out is not a reason to throw away a search in flight.
    if let Some(handle) = searching {
        handle.join().expect("search thread panicked");
    }
}

fn wait(searching: &mut Option<JoinHandle<()>>, stop: &AtomicBool) {
    if let Some(handle) = searching.take() {
        stop.store(true, Ordering::Relaxed);
        handle.join().expect("search thread panicked");
    }
}

fn go(
    mut position: Position,
    limits: Limits,
    table: Arc<TranspositionTable>,
    history: Vec<u64>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let start = Instant::now();
        let report = search(&mut position, limits, &table, &history);
        let micros = start.elapsed().as_micros().max(1);
        println!(
            "info depth {} score {} nodes {} time {} nps {}",
            report.depth,
            report_score(report.score),
            report.nodes,
            micros / 1_000,
            report.nodes as u128 * 1_000_000 / micros
        );
        if let Some(mv) = report.best {
            println!("bestmove {mv}");
        }
    })
}

/// Mate scores count plies from the root, the protocol wants moves.
fn report_score(score: Score) -> String {
    let plies = MATE - score.abs();
    if plies <= MAX_MATE_PLIES {
        let moves = (plies + 1) / 2;
        format!("mate {}", if score < 0 { -moves } else { moves })
    } else {
        format!("cp {score}")
    }
}

fn parse_limits(arguments: &[&str], us: Color) -> Limits {
    if let Some(depth) = named(arguments, "depth") {
        return Limits::depth(depth.max(1) as u32);
    }
    if let Some(millis) = named(arguments, "movetime") {
        return Limits::time(Duration::from_millis(millis.saturating_sub(MOVE_OVERHEAD).max(1)));
    }
    match parse_clock(arguments, us) {
        Some(span) => Limits::time(span),
        None => Limits::infinite(),
    }
}

/// The value the GUI gave `name`.
fn named(arguments: &[&str], name: &str) -> Option<u64> {
    arguments
        .windows(2)
        .find(|pair| pair[0] == name)
        .and_then(|pair| pair[1].parse::<u64>().ok())
}

/// How much of the clock one move gets. A flat share plus most of the
/// increment, held clear of the flag by [`MOVE_OVERHEAD`].
fn parse_clock(arguments: &[&str], us: Color) -> Option<Duration> {
    let (clock, bonus) = if us.is_white() {
        ("wtime", "winc")
    } else {
        ("btime", "binc")
    };

    let remaining = named(arguments, clock)?;
    let increment = named(arguments, bonus).unwrap_or(0);
    let span = (remaining / 20 + increment / 2).min(remaining.saturating_sub(MOVE_OVERHEAD));
    Some(Duration::from_millis(span.max(1)))
}

/// `[startpos | fen <fen>] [moves <move>...]`, where anything unparsable leaves
/// the position the GUI last set.
fn parse_position(arguments: &[&str]) -> Option<(Position, Vec<u64>)> {
    let split = arguments
        .iter()
        .position(|&token| token == "moves")
        .unwrap_or(arguments.len());
    let (setup, moves) = arguments.split_at(split);

    let mut position = match *setup.first()? {
        "startpos" => Position::starting(),
        "fen" => setup[1..].join(" ").parse().ok()?,
        _ => return None,
    };

    let mut history = Vec::with_capacity(moves.len());
    for text in moves.iter().skip(1) {
        let mv = find_move(&position, text)?;
        history.push(position.hash());
        position.make_move(mv);
    }
    Some((position, history))
}
