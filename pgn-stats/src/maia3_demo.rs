/// maia3-demo: for every game in a PGN file, query both maia3 (at each ELO)
/// and Stockfish (at each configured node budget) for every move from
/// `--min_ply` onwards and report how often each engine's best move matched
/// the move actually played.
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
const STOCKFISH_PATH: &str = "/usr/local/bin/stockfish";

const MAIA3_NODES: u32 = 1;
const ELOS: &[u32] = &[1200, 1600, 1800, 2000, 2200, 2400, 2600, 2800];

// Stockfish (name, nodes). Names appear in the column header and accuracy summary.
const STOCKFISH_RUNS: &[(&str, u32)] = &[
    ("1k",   1_000),
    ("10k",  10_000),
    ("100k", 100_000),
    ("1M",   1_000_000),
];

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

/// Get a SAN-rendered prediction for `pos` from `engine`, after caller has
/// already issued the appropriate `position fen ...` and any per-engine
/// option-setting.
fn predict_san(engine: &mut Engine, pos: &Chess, nodes: u32) -> io::Result<(String, String)> {
    let best_uci = engine.best_move(nodes)?;
    let san = best_uci.parse::<UciMove>().ok()
        .and_then(|um| um.to_move(pos).ok())
        .map(|pm| San::from_move(pos, pm).to_string())
        .unwrap_or_else(|| best_uci.clone());
    Ok((best_uci, san))
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

    println!("PGN file   : {pgn_file}");
    println!("Maia3      : {MAIA3_PATH}");
    println!("Stockfish  : {STOCKFISH_PATH}");
    println!("Min ply    : {min_ply}");
    println!("Maia3 ELOs : {ELOS:?}");
    print!  ("SF runs    : [");
    for (idx, (name, nodes)) in STOCKFISH_RUNS.iter().enumerate() {
        if idx > 0 { print!(", "); }
        print!("{name}={nodes}");
    }
    println!("]");
    println!("Games      : {}", games.len());
    println!();

    // ── Spin up engines ─────────────────────────────────────────────────────
    let mut maia = Engine::new(MAIA3_PATH)?;
    maia.init()?;
    maia.send("setoption name MultiPV value 1")?;
    maia.ensure_ready()?;

    let mut sf = Engine::new(STOCKFISH_PATH)?;
    sf.init()?;
    sf.send("setoption name MultiPV value 1")?;
    sf.ensure_ready()?;

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

        // Per-engine-cell totals & matches for this game, split by side to
        // move (White = 0, Black = 1).
        let mut elo_totals:  [Vec<usize>; 2] = [vec![0; ELOS.len()], vec![0; ELOS.len()]];
        let mut elo_matches: [Vec<usize>; 2] = [vec![0; ELOS.len()], vec![0; ELOS.len()]];
        let mut sf_totals:   [Vec<usize>; 2] = [vec![0; STOCKFISH_RUNS.len()], vec![0; STOCKFISH_RUNS.len()]];
        let mut sf_matches:  [Vec<usize>; 2] = [vec![0; STOCKFISH_RUNS.len()], vec![0; STOCKFISH_RUNS.len()]];

        // Header. Each engine-cell = 10 chars (2 leading + 7 SAN + 1 marker).
        print!("  {:>4}  {:<8}", "ply", "played");
        for elo in ELOS { print!("  {:>7} ", elo); }
        for (name, _) in STOCKFISH_RUNS { print!("  {:>7} ", name); }
        println!();

        for (ply, san_str) in game.moves.iter().enumerate() {
            let san: San = match san_str.parse() { Ok(s) => s, Err(_) => break };
            let m = match san.to_move(&pos) { Ok(m) => m, Err(_) => break };
            let actual_uci = UciMove::from_move(m.clone(), CastlingMode::Standard).to_string();
            let actual_san = San::from_move(&pos, m.clone()).to_string();

            if ply >= min_ply {
                let fen = Fen::from_position(&pos, shakmaty::EnPassantMode::Legal).to_string();
                let side = pos.turn() as usize; // White = 0, Black = 1

                // ── maia3 sweep ────────────────────────────────────────────
                maia.send(&format!("position fen {fen}"))?;
                let mut maia_preds: Vec<String> = Vec::with_capacity(ELOS.len());
                for (eidx, &elo) in ELOS.iter().enumerate() {
                    maia.send(&format!("setoption name Elo value {elo}"))?;
                    maia.ensure_ready()?;
                    let (best_uci, best_san) = predict_san(&mut maia, &pos, MAIA3_NODES)?;
                    if best_uci == actual_uci { elo_matches[side][eidx] += 1; }
                    elo_totals[side][eidx] += 1;
                    maia_preds.push(best_san);
                }

                // ── Stockfish sweep ────────────────────────────────────────
                sf.send(&format!("position fen {fen}"))?;
                let mut sf_preds: Vec<String> = Vec::with_capacity(STOCKFISH_RUNS.len());
                for (sidx, (_name, nodes)) in STOCKFISH_RUNS.iter().enumerate() {
                    let (best_uci, best_san) = predict_san(&mut sf, &pos, *nodes)?;
                    if best_uci == actual_uci { sf_matches[side][sidx] += 1; }
                    sf_totals[side][sidx] += 1;
                    sf_preds.push(best_san);
                }

                // ── Emit row ───────────────────────────────────────────────
                print!("  {:>4}  {:<8}", ply, actual_san);

                // maia3 columns: collapse runs to "." per spec.
                let mut prev: Option<&String> = None;
                for p in &maia_preds {
                    if prev == Some(p) {
                        print!("  {:>7} ", ".");
                    } else {
                        let marker = if p == &actual_san { "*" } else { " " };
                        print!("  {:>7}{}", p, marker);
                    }
                    prev = Some(p);
                }

                // Stockfish columns: collapse runs to "." per spec.
                let mut prev: Option<&String> = None;
                for p in &sf_preds {
                    if prev == Some(p) {
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
        }

        // Per-game accuracy summary, split by side.
        println!();
        for (side, side_name) in [(0usize, "White"), (1usize, "Black")] {
            println!("  Accuracy ({side_name}):");
            for (eidx, &elo) in ELOS.iter().enumerate() {
                let t = elo_totals[side][eidx];
                let h = elo_matches[side][eidx];
                if t == 0 {
                    println!("    Maia3 {:>4}     : n/a", elo);
                } else {
                    println!("    Maia3 {:>4}     : {:>5.1}% ({}/{})",
                             elo, 100.0 * h as f64 / t as f64, h, t);
                }
            }
            for (sidx, (name, _)) in STOCKFISH_RUNS.iter().enumerate() {
                let t = sf_totals[side][sidx];
                let h = sf_matches[side][sidx];
                if t == 0 {
                    println!("    Stockfish {:<4} : n/a", name);
                } else {
                    println!("    Stockfish {:<4} : {:>5.1}% ({}/{})",
                             name, 100.0 * h as f64 / t as f64, h, t);
                }
            }
        }
        println!();
    }

    maia.quit()?;
    sf.quit()
}
