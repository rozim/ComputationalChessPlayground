/// stockfish-variety: like stockfish-demo but plays variety chess.
///
/// At each turn, asks Stockfish for the top MULTI_PV moves. Any move whose
/// evaluation is within THRESHOLD centipawns of the best move is considered
/// "near-best". One of those is chosen at random (uniform), so the best move
/// is always a candidate but not always played.
///
/// Usage: stockfish-variety [--nodes N] [--threshold N]
///   --nodes N      Stockfish node budget per move (default: 1500000)
///   --threshold N  centipawn window for near-best moves (default: 25)

use std::env;
use std::io;

use pgn_stats::uci_engine::Engine;
use rand::seq::IndexedRandom;
use shakmaty::{Chess, Color, KnownOutcome, Outcome, Position, san::San, uci::UciMove};

const STOCKFISH_PATH: &str = "/usr/local/bin/stockfish";
const MULTI_PV: usize = 5;

// ── Arg parsing ───────────────────────────────────────────────────────────────

const DEFAULT_NODES: u32 = 1_500_000;
const DEFAULT_HASH_MB: u32 = 256;

struct Args {
    nodes: u32,
    hash_mb: u32,
    threshold: i32,
}

fn parse_args() -> Args {
    let mut nodes: u32 = DEFAULT_NODES;
    let mut hash_mb: u32 = DEFAULT_HASH_MB;
    let mut threshold: i32 = 25;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--nodes" | "-n" => {
                nodes = args.next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| { eprintln!("--nodes requires a value"); std::process::exit(1); });
            }
            "--hash" | "-H" => {
                hash_mb = args.next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| { eprintln!("--hash requires a value"); std::process::exit(1); });
            }
            "--threshold" | "-t" => {
                threshold = args.next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| { eprintln!("--threshold requires a value"); std::process::exit(1); });
            }
            "--help" | "-h" => {
                println!("Usage: stockfish-variety [--nodes N] [--hash MB] [--threshold N]");
                println!("  --nodes N      Stockfish node budget per move (default: {DEFAULT_NODES})");
                println!("  --hash MB      hash table size in MiB (default: {DEFAULT_HASH_MB})");
                println!("  --threshold N  centipawn window for near-best moves (default: 25)");
                std::process::exit(0);
            }
            other => {
                eprintln!("Unknown argument: {other}");
                std::process::exit(1);
            }
        }
    }

    Args { nodes, hash_mb, threshold }
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() -> io::Result<()> {
    let Args { nodes, hash_mb, threshold } = parse_args();

    let mut engine = Engine::new(STOCKFISH_PATH)?;
    engine.init()?;
    engine.set_hash(hash_mb)?;
    engine.send("position startpos")?;

    let mut pos = Chess::default();
    let mut played_uci: Vec<String> = Vec::new();
    let mut move_number = 1;
    let mut rng = rand::rng();

    // best_moves[0] = white, [1] = black
    let mut best_moves = [0usize; 2];
    let mut non_best_moves = [0usize; 2];

    println!("Stockfish variety (nodes={nodes}, hash={hash_mb}MiB, multi_pv={MULTI_PV}, threshold={threshold}cp)");
    println!();

    loop {
        let candidates = engine.go_multipv(nodes, MULTI_PV)?;
        if candidates.is_empty() {
            break;
        }

        let best_score = candidates[0].1;

        // Keep moves within `threshold` cp of the best.
        let near_best: Vec<&(String, i32)> = candidates
            .iter()
            .filter(|(_, score)| best_score - score <= threshold)
            .collect();

        let chosen = near_best
            .choose(&mut rng)
            .expect("at least one candidate");
        let uci_str = &chosen.0;

        // Track best vs non-best for each player.
        let player = pos.turn() as usize; // White=0, Black=1
        if uci_str == &candidates[0].0 {
            best_moves[player] += 1;
        } else {
            non_best_moves[player] += 1;
        }

        // Resolve to a shakmaty move and format as SAN.
        let uci_move: UciMove = uci_str.parse().unwrap_or_else(|e| {
            panic!("Bad UCI move '{uci_str}': {e}");
        });
        let m = uci_move.to_move(&pos).unwrap_or_else(|e| {
            panic!("Illegal move '{uci_str}': {e}");
        });
        let san = San::from_move(&pos, m.clone());

        if pos.turn() == Color::White {
            print!("{move_number:3}. {san:<8}");
        } else {
            println!(" {san}");
            move_number += 1;
        }

        pos.play_unchecked(m);
        played_uci.push(uci_str.clone());

        // Keep engine in sync.
        let moves_str = played_uci.join(" ");
        engine.send(&format!("position startpos moves {moves_str}"))?;

        if pos.is_game_over() {
            break;
        }
    }

    // Flush trailing half-line if game ended on White's move.
    if pos.turn() == Color::Black {
        println!();
    }
    println!();

    match pos.outcome() {
        Outcome::Known(KnownOutcome::Decisive { winner: Color::White }) => println!("Result: 1-0"),
        Outcome::Known(KnownOutcome::Decisive { winner: Color::Black }) => println!("Result: 0-1"),
        Outcome::Known(KnownOutcome::Draw) => println!("Result: 1/2-1/2"),
        _ => println!("Result: *"),
    }

    println!();
    println!("{:<10}  {:>9}  {:>9}", "", "best", "non-best");
    println!("{:<10}  {:>9}  {:>9}", "White", best_moves[0], non_best_moves[0]);
    println!("{:<10}  {:>9}  {:>9}", "Black", best_moves[1], non_best_moves[1]);

    engine.quit()
}
