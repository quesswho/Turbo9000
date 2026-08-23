use std::io::{self, BufRead};
use std::time::{Duration, Instant};

use engine::movegen::find_move;
use engine::perft::perft;
use engine::position::{Color, Position};
use engine::search::{search, Limits};
use engine::NAME;

/// Slack left on the clock so that reporting a move never flags.
const MOVE_OVERHEAD: u64 = 50;

fn main() {
    let mut position = Position::starting();

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
            "ucinewgame" => position = Position::starting(),
            "position" => {
                if let Some(parsed) = parse_position(arguments) {
                    position = parsed;
                }
            }
            "go" => match arguments {
                ["perft", depth] => {
                    if let Ok(depth) = depth.parse() {
                        println!("Nodes searched: {}", perft(&mut position, depth));
                    }
                }
                ["depth", depth] => {
                    if let Ok(depth) = depth.parse() {
                        go(&mut position, Limits::depth(depth));
                    }
                }
                _ => {
                    if let Some(span) = parse_clock(arguments, position.side_to_move()) {
                        go(&mut position, Limits::time(span));
                    }
                }
            },
            "quit" => break,
            _ => {}
        }
    }
}

fn go(position: &mut Position, limits: Limits) {
    let start = Instant::now();
    let report = search(position, limits);
    let micros = start.elapsed().as_micros().max(1);
    println!(
        "info depth {} nodes {} time {} nps {}",
        report.depth,
        report.nodes,
        micros / 1_000,
        report.nodes as u128 * 1_000_000 / micros
    );
    if let Some(mv) = report.best {
        println!("bestmove {mv}");
    }
}

/// How much of the clock one move gets. A flat share plus most of the
/// increment, held clear of the flag by [`MOVE_OVERHEAD`].
fn parse_clock(arguments: &[&str], us: Color) -> Option<Duration> {
    let (clock, bonus) = if us.is_white() {
        ("wtime", "winc")
    } else {
        ("btime", "binc")
    };

    let millis = |name: &str| {
        arguments
            .windows(2)
            .find(|pair| pair[0] == name)
            .and_then(|pair| pair[1].parse::<u64>().ok())
    };

    let remaining = millis(clock)?;
    let increment = millis(bonus).unwrap_or(0);
    let span = (remaining / 20 + increment / 2).min(remaining.saturating_sub(MOVE_OVERHEAD));
    Some(Duration::from_millis(span.max(1)))
}

/// `[startpos | fen <fen>] [moves <move>...]`, where anything unparsable leaves
/// the position the GUI last set.
fn parse_position(arguments: &[&str]) -> Option<Position> {
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

    for text in moves.iter().skip(1) {
        let mv = find_move(&position, text)?;
        position.make_move(mv);
    }
    Some(position)
}
