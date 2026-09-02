use std::collections::HashSet;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Instant;

use engine::movegen::{generate_all, MoveList};
use engine::position::{Black, Position, White};
use engine::search::{search, Limits, Score};
use engine::ttable::TranspositionTable;

use datagen::rng::Rng;

/// Random plies played from the start position to open a line.
const OPENING_PLIES: usize = 8;

/// An opening further from equal than this hands one side the game before the
/// engines have played a move.
const BALANCE: Score = 150;

/// Per thread, so the table never has to be shared.
const TABLE_MB: usize = 8;

const PROGRESS: u64 = 1_000;

fn legal(position: &Position, list: &mut MoveList) -> bool {
    if position.side_to_move().is_white() {
        generate_all::<White>(position, list)
    } else {
        generate_all::<Black>(position, list)
    }
}

/// Picks uniformly among the legal moves, so no engine's opening repertoire
/// steers which lines the book covers.
fn walk(rng: &mut Rng, list: &mut MoveList) -> Option<(Position, Vec<u64>)> {
    let mut position = Position::starting();
    let mut history = Vec::new();
    while history.len() < OPENING_PLIES {
        legal(&position, list);
        if list.is_empty() {
            return None;
        }
        let mv = list.moves()[rng.below(list.len())];
        history.push(position.hash());
        position.make_move(mv);
    }
    legal(&position, list);
    if list.is_empty() {
        return None;
    }
    Some((position, history))
}

fn main() {
    let mut arguments = std::env::args().skip(1);
    let (Some(output), Some(target)) = (arguments.next(), arguments.next()) else {
        eprintln!("usage: bookgen <output> <openings> [threads] [depth] [seed]");
        std::process::exit(2);
    };
    let target: u64 = target.parse().expect("openings is not a number");
    let threads: usize = match arguments.next() {
        Some(text) => text.parse().expect("threads is not a number"),
        None => thread::available_parallelism().map_or(1, |count| count.get()),
    };
    let depth: u32 = match arguments.next() {
        Some(text) => text.parse().expect("depth is not a number"),
        None => 8,
    };
    let seed: u64 = match arguments.next() {
        Some(text) => text.parse().expect("seed is not a number"),
        None => 0x2545_F491_4F6C_DD1D,
    };

    let file = File::create(&output).expect("cannot create the output file");
    let writer = Mutex::new(BufWriter::new(file));
    let seen = Mutex::new(HashSet::new());
    let written = AtomicU64::new(0);
    let reported = AtomicU64::new(0);
    let start = Instant::now();

    thread::scope(|scope| {
        for index in 0..threads {
            let writer = &writer;
            let seen = &seen;
            let written = &written;
            let reported = &reported;
            scope.spawn(move || {
                let table = TranspositionTable::new(TABLE_MB);
                let mut rng = Rng::new(seed ^ (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
                let mut list = MoveList::new();
                while written.load(Ordering::Relaxed) < target {
                    let Some((mut position, history)) = walk(&mut rng, &mut list) else {
                        continue;
                    };
                    // A line rejected below stays in the set, so no thread
                    // pays to search it twice.
                    if !seen.lock().expect("the set is poisoned").insert(position.hash()) {
                        continue;
                    }

                    table.clear();
                    let report = search(&mut position, Limits::depth(depth), &table, &history);
                    if report.score.abs() > BALANCE {
                        continue;
                    }

                    let fen = position.to_fen();
                    let mut writer = writer.lock().expect("the writer is poisoned");
                    writeln!(writer, "{fen}").expect("cannot write an opening");
                    drop(writer);

                    let total = written.fetch_add(1, Ordering::Relaxed) + 1;
                    let milestone = total / PROGRESS;
                    if milestone > reported.swap(milestone, Ordering::Relaxed) {
                        let elapsed = start.elapsed().as_secs_f64().max(1e-9);
                        eprintln!("{total} openings, {:.0}/s", total as f64 / elapsed);
                    }
                }
            });
        }
    });

    writer
        .lock()
        .expect("the writer is poisoned")
        .flush()
        .expect("cannot flush the output file");
    eprintln!(
        "wrote {} openings to {output} in {:.1}s",
        written.load(Ordering::Relaxed),
        start.elapsed().as_secs_f64()
    );
}
