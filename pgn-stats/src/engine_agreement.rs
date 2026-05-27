/// engine_agreement: for every game in a PGN file, compute how often each
/// player's moves matched Stockfish's best move at a given depth.
///
/// Repeated positions within a game are deduplicated — only the last
/// occurrence of each (pieces + turn + castling) triplet is evaluated.
/// The first `min_ply` half-moves of each game are skipped.
///
/// Usage: engine_agreement <file.pgn> [--depth N] [--min-ply N] [--event EVENT]

use std::collections::HashMap;
use std::env;
use std::io;

use pgn_game_reader::read_games;
use pgn_stats::uci_engine::Engine;
use shakmaty::{
    CastlingMode, Chess, Color, EnPassantMode, Position,
    fen::Fen,
    san::San,
    uci::UciMove,
};

const STOCKFISH_PATH: &str = "/usr/local/bin/stockfish";

// ── Arg parsing ───────────────────────────────────────────────────────────────

struct Args {
    pgn_file: String,
    depth: u32,
    min_ply: usize,
    event: Option<String>,
}

fn parse_args() -> Args {
    let mut pgn_file: Option<String> = None;
    let mut depth: u32 = 15;
    let mut min_ply: usize = 0;
    let mut event: Option<String> = None;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--depth" | "-d" => {
                depth = args.next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| { eprintln!("--depth requires a value"); std::process::exit(1); });
            }
            "--min-ply" | "-m" => {
                min_ply = args.next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| { eprintln!("--min-ply requires a value"); std::process::exit(1); });
            }
            "--event" | "-e" => {
                event = Some(args.next().unwrap_or_else(|| {
                    eprintln!("--event requires a value");
                    std::process::exit(1);
                }));
            }
            "--help" | "-h" => {
                println!("Usage: engine_agreement <file.pgn> [--depth N] [--min-ply N] [--event EVENT]");
                println!("  --depth N      Stockfish search depth (default: 15)");
                println!("  --min-ply N    skip first N half-moves of each game (default: 0)");
                println!("  --event EVENT  only analyse games whose Event tag matches EVENT");
                std::process::exit(0);
            }
            other if !other.starts_with('-') => {
                pgn_file = Some(other.to_owned());
            }
            other => {
                eprintln!("Unknown argument: {other}");
                std::process::exit(1);
            }
        }
    }

    let pgn_file = pgn_file.unwrap_or_else(|| {
        eprintln!("Usage: engine_agreement <file.pgn> [--depth N] [--min-ply N] [--event EVENT]");
        std::process::exit(1);
    });

    Args { pgn_file, depth, min_ply, event }
}

// ── Position key ──────────────────────────────────────────────────────────────

/// Returns a position key using only pieces, side-to-move, and castling rights
/// (the first three space-separated fields of FEN), ignoring en passant and
/// move counters.
fn position_key(pos: &Chess) -> String {
    let fen = Fen::from_position(pos, EnPassantMode::Always).to_string();
    let mut fields = fen.splitn(4, ' ');
    format!(
        "{} {} {}",
        fields.next().unwrap_or(""),
        fields.next().unwrap_or(""),
        fields.next().unwrap_or(""),
    )
}

// ── Per-position record ───────────────────────────────────────────────────────

struct PosRecord {
    full_fen: String,   // for Stockfish
    color: Color,       // side that moved
    actual_uci: String, // what was actually played
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() -> io::Result<()> {
    let Args { pgn_file, depth, min_ply, event } = parse_args();

    let mut games = match read_games(&pgn_file) {
        Ok(g) => g,
        Err(e) => { eprintln!("Error reading {pgn_file}: {e}"); std::process::exit(1); }
    };

    if let Some(ref event_str) = event {
        games.retain(|g| g.tags.get("Event").map(|e| e == event_str).unwrap_or(false));
        if games.is_empty() {
            eprintln!("No games found with Event = \"{event_str}\" in {pgn_file}");
            std::process::exit(1);
        }
    }

    if games.is_empty() {
        eprintln!("No games found in {pgn_file}");
        std::process::exit(1);
    }

    // Print configuration
    println!("PGN file : {pgn_file}");
    println!("Depth    : {depth}");
    println!("Min ply  : {min_ply}");
    if let Some(ref event_str) = event {
        println!("Event    : {event_str}");
    }
    println!("Games    : {}", games.len());
    println!();

    let mut engine = Engine::new(STOCKFISH_PATH)?;
    engine.init()?;

    // Header
    println!(
        "{:>5}  {:<30} {:<30}  {:<7}  {:>12}  {:>12}",
        "Round", "White", "Black", "Result", "White agree", "Black agree"
    );
    println!("{}", "-".repeat(107));

    for game in &games {
        let white  = game.tags.get("White").map(|s| s.as_str()).unwrap_or("?");
        let black  = game.tags.get("Black").map(|s| s.as_str()).unwrap_or("?");
        let round  = game.tags.get("Round").map(|s| s.as_str()).unwrap_or("?");
        let result = game.tags.get("Result").map(|s| s.as_str()).unwrap_or("?");

        // Build starting position (handle custom FEN via SetUp tag).
        let mut pos: Chess = if game.tags.get("SetUp").map(|s| s == "1").unwrap_or(false) {
            match game.tags.get("FEN").and_then(|f| f.parse::<Fen>().ok()) {
                Some(fen) => match fen.into_position(CastlingMode::Standard) {
                    Ok(p) => p,
                    Err(_) => Chess::default(),
                },
                None => Chess::default(),
            }
        } else {
            Chess::default()
        };

        // Walk through the game, building a map of unique positions -> last move played.
        // key: position_key (pieces + turn + castling)
        // value: PosRecord (full FEN, color, actual UCI move)
        let mut positions: HashMap<String, PosRecord> = HashMap::new();

        for (ply, san_str) in game.moves.iter().enumerate() {
            let san: San = match san_str.parse() {
                Ok(s) => s,
                Err(_) => break,
            };
            let m = match san.to_move(&pos) {
                Ok(m) => m,
                Err(_) => break,
            };

            if ply >= min_ply {
                let key = position_key(&pos);
                let full_fen = Fen::from_position(&pos, EnPassantMode::Legal).to_string();
                let actual_uci = UciMove::from_move(m.clone(), CastlingMode::Standard).to_string();
                // Overwrite — last occurrence wins.
                positions.insert(key, PosRecord { full_fen, color: pos.turn(), actual_uci });
            }

            pos.play_unchecked(m);
        }

        // Query Stockfish for each unique position.
        let mut counts = [[0usize; 2]; 2]; // counts[color][0=total, 1=best]

        for record in positions.values() {
            engine.send(&format!("position fen {}", record.full_fen))?;
            let best = engine.best_move(depth)?;

            let c = record.color as usize; // White=0, Black=1
            counts[c][0] += 1;
            if record.actual_uci == best {
                counts[c][1] += 1;
            }
        }

        let pct = |c: usize| -> String {
            let total = counts[c][0];
            let best  = counts[c][1];
            if total == 0 {
                "  n/a".to_owned()
            } else {
                format!("{:.1}% ({}/{})", 100.0 * best as f64 / total as f64, best, total)
            }
        };

        println!(
            "{:>5}  {:<30} {:<30}  {:<7}  {:>12}  {:>12}",
            round, white, black, result, pct(0), pct(1)
        );
    }

    engine.quit()
}
