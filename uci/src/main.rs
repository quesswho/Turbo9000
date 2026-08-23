use std::io::{self, BufRead};

use engine::movegen::find_move;
use engine::perft::perft;
use engine::position::Position;
use engine::search::search;
use engine::NAME;

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
                        if let Some(mv) = search(&mut position, depth) {
                            println!("bestmove {mv}");
                        }
                    }
                }
                _ => {}
            },
            "quit" => break,
            _ => {}
        }
    }
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
