/// maia3-demo: for every game in a PGN file, query maia3 at each ELO setting
/// for every move (from `--min_ply` onwards) and report how often each ELO's
/// best move matched the move actually played.
///
/// Usage: maia3-demo [--min_ply N] <file.pgn>

use std::env;
use std::io;

use pgn_game_reader::read_games;
use pgn_stats::uci_engine::Engine;
use shakmaty::{
    CastlingMode, Chess, Position,
    fen::Fen,
    san::San,
    uci::UciMove,
};

const MAIA3_PATH: &str = "/Users/dave/venv-maia3/bin/maia3-23m";
const NODES: u32 = 1;
const ELOS: &[u32] = &[600, 800, 1000, 1200, 1400, 1600, 1800, 2000, 2200, 2400, 2600, 2800];

const DEFAULT_MIN_PLY: usize = 10;

struct Args {
    pgn_file: String,
    min_ply: usize,
}

fn parse_args() -> Args {
    let mut pgn_file: Option<String> = None;
    let mut min_ply: usize = DEFAULT_MIN_PLY;

    let raw: Vec<String> = env::args().skip(1).collect();
    let mut i = 0;
    while i < raw.len() {
        let a = &raw[i];

        // Allow --min_ply=N / --min-ply=N
        if let Some(rest) = a.strip_prefix("--min_ply=").or_else(|| a.strip_prefix("--min-ply=")) {
            min_ply = rest.parse().unwrap_or_else(|_| {
                eprintln!("--min_ply requires an integer value"); std::process::exit(1);
            });
            i += 1;
            continue;
        }

        match a.as_str() {
            "--min_ply" | "--min-ply" | "-m" => {
                i += 1;
                min_ply = raw.get(i)
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| {
                        eprintln!("--min_ply requires a value"); std::process::exit(1);
                    });
                i += 1;
            }
            "--help" | "-h" => {
                println!("Usage: maia3-demo [--min_ply N] <file.pgn>");
                println!("  --min_ply N  skip first N half-moves of each game (default: {DEFAULT_MIN_PLY})");
                std::process::exit(0);
            }
            other if !other.starts_with('-') => {
                pgn_file = Some(other.to_owned());
                i += 1;
            }
            other => {
                eprintln!("Unknown argument: {other}");
                std::process::exit(1);
            }
        }
    }

    let pgn_file = pgn_file.unwrap_or_else(|| {
        eprintln!("Usage: maia3-demo [--min_ply N] <file.pgn>");
        std::process::exit(1);
    });

    Args { pgn_file, min_ply }
}

fn main() -> io::Result<()> {
    let Args { pgn_file, min_ply } = parse_args();

    let games = match read_games(&pgn_file) {
        Ok(g) => g,
        Err(e) => { eprintln!("Error reading {pgn_file}: {e}"); std::process::exit(1); }
    };
    if games.is_empty() {
        eprintln!("No games found in {pgn_file}");
        std::process::exit(1);
    }

    println!("PGN file : {pgn_file}");
    println!("Engine   : {MAIA3_PATH}");
    println!("Nodes    : {NODES}");
    println!("Min ply  : {min_ply}");
    println!("ELOs     : {ELOS:?}");
    println!("Games    : {}", games.len());
    println!();

    let mut engine = Engine::new(MAIA3_PATH)?;
    engine.init()?;
    engine.send("setoption name MultiPV value 1")?;
    engine.ensure_ready()?;

    for (gidx, game) in games.iter().enumerate() {
        let white  = game.tags.get("White").map(|s| s.as_str()).unwrap_or("?");
        let black  = game.tags.get("Black").map(|s| s.as_str()).unwrap_or("?");
        let round  = game.tags.get("Round").map(|s| s.as_str()).unwrap_or("?");
        let result = game.tags.get("Result").map(|s| s.as_str()).unwrap_or("?");

        println!("── Game {} ({}): {} vs {} [{}] ──",
                 gidx + 1, round, white, black, result);

        // Build starting position (handle SetUp/FEN tag).
        let mut pos: Chess = if game.tags.get("SetUp").map(|s| s == "1").unwrap_or(false) {
            match game.tags.get("FEN").and_then(|f| f.parse::<Fen>().ok()) {
                Some(fen) => fen.into_position(CastlingMode::Standard).unwrap_or_default(),
                None => Chess::default(),
            }
        } else {
            Chess::default()
        };

        // Per-ELO totals & matches for this game.
        let mut totals: Vec<usize> = vec![0; ELOS.len()];
        let mut matches: Vec<usize> = vec![0; ELOS.len()];

        // Print column header: ply, played, then one column per ELO.
        // Per-ELO cell width = 2 leading + 7 SAN + 1 marker = 10 chars.
        print!("  {:>4}  {:<8}", "ply", "played");
        for elo in ELOS { print!("  {:>7} ", elo); }
        println!();

        let mut played_uci_history: Vec<String> = Vec::new();

        for (ply, san_str) in game.moves.iter().enumerate() {
            let san: San = match san_str.parse() { Ok(s) => s, Err(_) => break };
            let m = match san.to_move(&pos) { Ok(m) => m, Err(_) => break };
            let actual_uci = UciMove::from_move(m.clone(), CastlingMode::Standard).to_string();
            let actual_san = San::from_move(&pos, m.clone()).to_string();

            if ply >= min_ply {
                // Position the engine at the current FEN.
                let fen = Fen::from_position(&pos, shakmaty::EnPassantMode::Legal).to_string();
                engine.send(&format!("position fen {fen}"))?;

                let mut preds_san: Vec<String> = Vec::with_capacity(ELOS.len());
                for (eidx, &elo) in ELOS.iter().enumerate() {
                    engine.send(&format!("setoption name Elo value {elo}"))?;
                    engine.ensure_ready()?;
                    let best_uci = engine.best_move(NODES)?;
                    if best_uci == actual_uci {
                        matches[eidx] += 1;
                    }
                    totals[eidx] += 1;

                    // Convert engine's UCI prediction to SAN relative to current pos.
                    let san_pred = best_uci.parse::<UciMove>().ok()
                        .and_then(|um| um.to_move(&pos).ok())
                        .map(|pm| San::from_move(&pos, pm).to_string())
                        .unwrap_or(best_uci);
                    preds_san.push(san_pred);
                }

                print!("  {:>4}  {:<8}", ply, actual_san);
                let mut prev: Option<&String> = None;
                for p in &preds_san {
                    if prev == Some(p) {
                        // Same as previous ELO's prediction — print a single ".".
                        print!("  {:>7} ", ".");
                    } else {
                        let marker = if p == &actual_san { "*" } else { " " };
                        print!("  {:>7}{}", p, marker);
                    }
                    prev = Some(p);
                }
                println!();
            }

            pos.play_unchecked(m);
            played_uci_history.push(actual_uci);
        }

        // Per-game accuracy summary.
        println!();
        println!("  Accuracy:");
        for (eidx, &elo) in ELOS.iter().enumerate() {
            let t = totals[eidx];
            let h = matches[eidx];
            if t == 0 {
                println!("    Elo {:>4}: n/a", elo);
            } else {
                println!("    Elo {:>4}: {:>5.1}% ({}/{})",
                         elo, 100.0 * h as f64 / t as f64, h, t);
            }
        }
        println!();
    }

    engine.quit()
}
