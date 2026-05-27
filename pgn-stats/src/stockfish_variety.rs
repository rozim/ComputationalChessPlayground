/// stockfish-variety: like stockfish-demo but plays variety chess.
///
/// At each turn, asks Stockfish for the top MULTI_PV moves. Any move whose
/// evaluation is within THRESHOLD centipawns of the best move is considered
/// "near-best". One of those is chosen at random (uniform), so the best move
/// is always a candidate but not always played.
///
/// Usage: stockfish-variety [threshold_cp]   (default threshold: 25)

use std::collections::HashMap;
use std::env;
use std::io::{self, BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use rand::seq::IndexedRandom;
use shakmaty::{Chess, Color, KnownOutcome, Outcome, Position, san::San, uci::UciMove};

const STOCKFISH_PATH: &str = "/usr/local/bin/stockfish";
const DEPTH: u32 = 5;
const MULTI_PV: usize = 5;

// ── Minimal UCI engine wrapper ────────────────────────────────────────────────

struct Engine {
    stdin: ChildStdin,
    receiver: Receiver<String>,
    _child: Child,
}

impl Engine {
    fn new(path: &str) -> io::Result<Self> {
        let mut child = Command::new(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let stdin = child.stdin.take().expect("stdin piped");
        let stdout = child.stdout.take().expect("stdout piped");

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(l) => { let _ = tx.send(l); }
                    Err(_) => break,
                }
            }
        });

        Ok(Engine { stdin, receiver: rx, _child: child })
    }

    fn send(&mut self, cmd: &str) -> io::Result<()> {
        writeln!(self.stdin, "{cmd}")?;
        self.stdin.flush()
    }

    fn recv(&self) -> String {
        self.receiver.recv().unwrap_or_default()
    }

    fn ensure_ready(&mut self) -> io::Result<()> {
        self.send("isready")?;
        loop {
            if self.recv() == "readyok" { break; }
        }
        Ok(())
    }

    /// Ask for top `n` moves at `depth`. Returns Vec<(uci_move, centipawn_score)>
    /// sorted best-first (highest score first). Scores are from the perspective
    /// of the side to move; mate scores are mapped to ±100_000 cp.
    fn go_multipv(&mut self, depth: u32, n: usize) -> io::Result<Vec<(String, i32)>> {
        self.send(&format!("setoption name MultiPV value {n}"))?;
        self.send(&format!("go depth {depth}"))?;

        // key = multipv index (1-based); value = (uci_move, score, depth_seen)
        let mut by_pv: HashMap<usize, (String, i32, u32)> = HashMap::new();

        loop {
            let line = self.recv();
            if line.starts_with("bestmove") {
                break;
            }
            // Only care about multipv info lines with a pv move.
            if !line.contains("multipv") || !line.contains(" pv ") {
                continue;
            }
            // Skip bound estimates (not exact scores).
            if line.contains("lowerbound") || line.contains("upperbound") {
                continue;
            }

            parse_info_line(&line, &mut by_pv);
        }

        let mut moves: Vec<(String, i32)> = by_pv.into_values()
            .map(|(m, s, _)| (m, s))
            .collect();
        moves.sort_by(|a, b| b.1.cmp(&a.1)); // best score first
        Ok(moves)
    }

    fn quit(&mut self) -> io::Result<()> {
        self.send("quit")
    }
}

/// Parse a single Stockfish `info … multipv … score … pv …` line.
/// Updates `by_pv` keyed by multipv index, keeping the deepest result.
fn parse_info_line(line: &str, by_pv: &mut HashMap<usize, (String, i32, u32)>) {
    let parts: Vec<&str> = line.split_ascii_whitespace().collect();
    let mut line_depth: Option<u32> = None;
    let mut multipv_idx: Option<usize> = None;
    let mut score: Option<i32> = None;
    let mut pv_move: Option<String> = None;

    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "depth" if i + 1 < parts.len() => {
                line_depth = parts[i + 1].parse().ok();
                i += 2;
            }
            "multipv" if i + 1 < parts.len() => {
                multipv_idx = parts[i + 1].parse().ok();
                i += 2;
            }
            "score" if i + 2 < parts.len() => {
                score = match parts[i + 1] {
                    "cp" => parts[i + 2].parse().ok(),
                    "mate" => parts[i + 2].parse::<i32>().ok().map(|n| {
                        if n > 0 { 100_000 } else { -100_000 }
                    }),
                    _ => None,
                };
                i += 3;
            }
            "pv" if i + 1 < parts.len() => {
                pv_move = Some(parts[i + 1].to_string());
                break; // pv is the last field we need
            }
            _ => { i += 1; }
        }
    }

    if let (Some(d), Some(idx), Some(s), Some(m)) = (line_depth, multipv_idx, score, pv_move) {
        let entry = by_pv.entry(idx).or_insert((String::new(), i32::MIN, 0));
        if d >= entry.2 {
            *entry = (m, s, d);
        }
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() -> io::Result<()> {
    let threshold: i32 = env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(25);

    let mut engine = Engine::new(STOCKFISH_PATH)?;

    // Handshake
    engine.send("uci")?;
    while engine.recv() != "uciok" {}
    engine.ensure_ready()?;
    engine.send("ucinewgame")?;
    engine.ensure_ready()?;
    engine.send("position startpos")?;

    let mut pos = Chess::default();
    let mut played_uci: Vec<String> = Vec::new();
    let mut move_number = 1;
    let mut rng = rand::rng();

    println!("Stockfish variety (depth={DEPTH}, multi_pv={MULTI_PV}, threshold={threshold}cp)");
    println!();

    loop {
        let candidates = engine.go_multipv(DEPTH, MULTI_PV)?;
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

    engine.quit()
}
