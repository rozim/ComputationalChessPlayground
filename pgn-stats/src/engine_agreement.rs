/// engine_agreement: for every game in a PGN file, compute how often each
/// player's moves matched Stockfish's best move at a given depth, and what
/// each player's accuracy percentage was (lichess method).
///
/// Repeated positions within a game are deduplicated for engine-agreement —
/// only the last occurrence of each (pieces + turn + castling) triplet is
/// counted.  Accuracy uses every position in sequence.
///
/// The first `min_ply` half-moves of each game are skipped for both metrics.
///
/// Usage: engine_agreement <file.pgn> [--depth N] [--min-ply N] [--event EVENT]

use std::collections::{HashMap, HashSet};
use std::env;
use std::io;

use pgn_game_reader::read_games;
use pgn_stats::accuracy::{game_accuracy, INITIAL_CP};
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
    nodes: u32,
    hash_mb: u32,
    min_ply: usize,
    event: Option<String>,
}

/// Default node budget matches lichess: lila/conf/base.conf `analysis.nodes = 1500000`.
const DEFAULT_NODES: u32 = 1_500_000;
const DEFAULT_HASH_MB: u32 = 256;

fn parse_args() -> Args {
    let mut pgn_file: Option<String> = None;
    let mut nodes: u32 = DEFAULT_NODES;
    let mut hash_mb: u32 = DEFAULT_HASH_MB;
    let mut min_ply: usize = 0;
    let mut event: Option<String> = None;

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
                println!("Usage: engine_agreement <file.pgn> [--nodes N] [--hash MB] [--min-ply N] [--event EVENT]");
                println!("  --nodes N      Stockfish node budget per position (default: {DEFAULT_NODES})");
                println!("  --hash MB      hash table size in MiB (default: {DEFAULT_HASH_MB})");
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
        eprintln!("Usage: engine_agreement <file.pgn> [--nodes N] [--hash MB] [--min-ply N] [--event EVENT]");
        std::process::exit(1);
    });

    Args { pgn_file, nodes, hash_mb, min_ply, event }
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

// ── Per-position records ──────────────────────────────────────────────────────

/// One entry in the agreement dedup map (last occurrence of each position key).
struct AgreementRecord {
    full_fen: String,
    color: Color,
    actual_uci: String,
}

/// One entry in the ordered accuracy sequence.
struct SeqRecord {
    full_fen: String,
    stm: Color, // side to move at this position
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Normalise a Stockfish score (side-to-move perspective) to white's perspective.
fn to_white_cp(score: i32, stm: Color) -> i32 {
    if stm == Color::White { score } else { -score }
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() -> io::Result<()> {
    let Args { pgn_file, nodes, hash_mb, min_ply, event } = parse_args();

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
    println!("Nodes    : {nodes}");
    println!("Hash     : {hash_mb} MiB");
    println!("Min ply  : {min_ply}");
    if let Some(ref event_str) = event {
        println!("Event    : {event_str}");
    }
    println!("Games    : {}", games.len());
    println!();

    let mut engine = Engine::new(STOCKFISH_PATH)?;
    engine.init()?;
    engine.set_hash(hash_mb)?;

    // ── Header ────────────────────────────────────────────────────────────────
    println!(
        "{:>5}  {:<30} {:<30}  {:<7}  {:>15}  {:>7}  {:>15}  {:>7}",
        "Round", "White", "Black", "Result",
        "White match", "W acc%", "Black match", "B acc%"
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

        // Walk through the game, collecting:
        //   `agreement` — one record per unique position key (last occurrence),
        //                 for engine-agreement %.
        //   `seq`       — every position in move order from min_ply onwards,
        //                 for accuracy calculation.
        let mut agreement: HashMap<String, AgreementRecord> = HashMap::new();
        let mut seq: Vec<SeqRecord> = Vec::new();

        for (ply, san_str) in game.moves.iter().enumerate() {
            let san: San = match san_str.parse() { Ok(s) => s, Err(_) => break };
            let m = match san.to_move(&pos) { Ok(m) => m, Err(_) => break };

            if ply >= min_ply {
                let key      = position_key(&pos);
                let full_fen = Fen::from_position(&pos, EnPassantMode::Legal).to_string();
                let actual_uci = UciMove::from_move(m.clone(), CastlingMode::Standard).to_string();
                let stm = pos.turn();
                agreement.insert(key, AgreementRecord { full_fen: full_fen.clone(), color: stm, actual_uci });
                seq.push(SeqRecord { full_fen, stm });
            }

            pos.play_unchecked(m);
        }

        // FEN of the final position (after the last move) — needed to close the
        // accuracy sequence.
        let final_fen = Fen::from_position(&pos, EnPassantMode::Legal).to_string();
        let final_stm = pos.turn();

        // ── Query Stockfish ───────────────────────────────────────────────────
        // Build the set of unique FENs that need evaluation, then query each once.
        let mut fens_needed: HashSet<String> = HashSet::new();
        for rec in agreement.values() { fens_needed.insert(rec.full_fen.clone()); }
        for rec in &seq               { fens_needed.insert(rec.full_fen.clone()); }
        if !seq.is_empty()            { fens_needed.insert(final_fen.clone()); }

        // cache: FEN → (best_uci_move, score_from_stm_perspective)
        let mut cache: HashMap<String, (String, i32)> = HashMap::new();
        for fen in &fens_needed {
            engine.send(&format!("position fen {fen}"))?;
            let (best, score) = engine.best_move_and_score(nodes)?;
            cache.insert(fen.clone(), (best, score));
        }

        // ── Engine agreement ──────────────────────────────────────────────────
        // counts[color][0] = total positions; counts[color][1] = best-move matches
        let mut counts = [[0usize; 2]; 2];
        for rec in agreement.values() {
            if let Some((best, _)) = cache.get(&rec.full_fen) {
                let c = rec.color as usize;
                counts[c][0] += 1;
                if rec.actual_uci == *best { counts[c][1] += 1; }
            }
        }

        let match_str = |c: usize| -> String {
            let total = counts[c][0];
            let hits  = counts[c][1];
            if total == 0 { "n/a".to_owned() }
            else { format!("{:.1}% ({}/{})", 100.0 * hits as f64 / total as f64, hits, total) }
        };

        // ── Accuracy ─────────────────────────────────────────────────────────
        let (white_acc, black_acc) = if seq.len() >= 2 {
            // initial_cp: score of the position before the first included move.
            let initial_cp = cache.get(&seq[0].full_fen)
                .map(|(_, s)| to_white_cp(*s, seq[0].stm))
                .unwrap_or(INITIAL_CP);

            // cps: white-perspective scores after each included move
            //      = score of position before the *next* move
            //      = seq[1], seq[2], ..., seq[N-1], final
            let cps: Vec<i32> = seq[1..].iter()
                .filter_map(|r| cache.get(&r.full_fen).map(|(_, s)| to_white_cp(*s, r.stm)))
                .chain(
                    cache.get(&final_fen)
                         .map(|(_, s)| to_white_cp(*s, final_stm))
                )
                .collect();

            let start_white = seq[0].stm == Color::White;
            match game_accuracy(start_white, initial_cp, &cps) {
                Some((wa, ba)) => (Some(wa), Some(ba)),
                None => (None, None),
            }
        } else {
            (None, None)
        };

        let acc_str = |a: Option<f64>| -> String {
            match a {
                Some(v) => format!("{:.1}%", v),
                None    => "n/a".to_owned(),
            }
        };

        println!(
            "{:>5}  {:<30} {:<30}  {:<7}  {:>15}  {:>7}  {:>15}  {:>7}",
            round, white, black, result,
            match_str(0), acc_str(white_acc),
            match_str(1), acc_str(black_acc),
        );
    }

    engine.quit()
}
