/// Minimal synchronous UCI engine wrapper.
///
/// Spawns a stockfish-compatible process and speaks the UCI protocol over
/// its stdin/stdout. All I/O runs on a background thread; the caller
/// communicates through the `send` / `recv` methods.

use std::io::{self, BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;

pub struct Engine {
    pub stdin: ChildStdin,
    pub receiver: Receiver<String>,
    _child: Child,
}

impl Engine {
    pub fn new(path: &str) -> io::Result<Self> {
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

    pub fn send(&mut self, cmd: &str) -> io::Result<()> {
        writeln!(self.stdin, "{cmd}")?;
        self.stdin.flush()
    }

    pub fn recv(&self) -> String {
        self.receiver.recv().unwrap_or_default()
    }

    pub fn ensure_ready(&mut self) -> io::Result<()> {
        self.send("isready")?;
        loop {
            if self.recv() == "readyok" { break; }
        }
        Ok(())
    }

    pub fn quit(&mut self) -> io::Result<()> {
        self.send("quit")
    }

    /// Perform UCI handshake and new-game init.
    pub fn init(&mut self) -> io::Result<()> {
        self.send("uci")?;
        while self.recv() != "uciok" {}
        self.ensure_ready()?;
        self.send("ucinewgame")?;
        self.ensure_ready()
    }

    /// Set the hash table size in MiB.  Call after `init` and before searching.
    /// Sends `isready`/`readyok` to confirm the engine has resized before returning.
    pub fn set_hash(&mut self, mb: u32) -> io::Result<()> {
        self.send(&format!("setoption name Hash value {mb}"))?;
        self.ensure_ready()
    }

    /// Clear the transposition table.  Call before each search for reproducible,
    /// position-independent results.
    fn clear_hash(&mut self) -> io::Result<()> {
        self.send("setoption name Clear Hash")
    }

    /// Ask for top `n` moves within a node budget. Returns `Vec<(uci_move, centipawn_score)>`
    /// sorted best-first. Mate scores map to ±100 000 cp.
    pub fn go_multipv(&mut self, nodes: u32, n: usize) -> io::Result<Vec<(String, i32)>> {
        use std::collections::HashMap;

        self.clear_hash()?;
        self.send(&format!("setoption name MultiPV value {n}"))?;
        self.send(&format!("go nodes {nodes}"))?;

        let mut by_pv: HashMap<usize, (String, i32, u32)> = HashMap::new();

        loop {
            let line = self.recv();
            if line.starts_with("bestmove") { break; }
            if !line.contains("multipv") || !line.contains(" pv ") { continue; }
            if line.contains("lowerbound") || line.contains("upperbound") { continue; }
            parse_info_line(&line, &mut by_pv);
        }

        let mut moves: Vec<(String, i32)> = by_pv.into_values()
            .map(|(m, s, _)| (m, s))
            .collect();
        moves.sort_by(|a, b| b.1.cmp(&a.1));
        Ok(moves)
    }

    /// Ask for the single best move within a node budget. Returns the UCI move string.
    pub fn best_move(&mut self, nodes: u32) -> io::Result<String> {
        self.clear_hash()?;
        self.send(&format!("setoption name MultiPV value 1"))?;
        self.send(&format!("go nodes {nodes}"))?;
        loop {
            let line = self.recv();
            if line.starts_with("bestmove") {
                let best = line.split_ascii_whitespace()
                    .nth(1)
                    .unwrap_or("none")
                    .to_owned();
                return Ok(best);
            }
        }
    }

    /// Ask for the single best move and its centipawn score within a node budget.
    ///
    /// The score is from the **side-to-move's** perspective (positive = the
    /// moving side is better), matching raw Stockfish UCI output.  The caller
    /// is responsible for negating when the side to move is black if a
    /// white-perspective score is needed.
    ///
    /// Returns `(uci_move, centipawns)`.
    pub fn best_move_and_score(&mut self, nodes: u32) -> io::Result<(String, i32)> {
        use std::collections::HashMap;
        self.clear_hash()?;
        self.send("setoption name MultiPV value 1")?;
        self.send(&format!("go nodes {nodes}"))?;
        let mut by_pv: HashMap<usize, (String, i32, u32)> = HashMap::new();
        loop {
            let line = self.recv();
            if line.starts_with("bestmove") {
                let best = line.split_ascii_whitespace()
                    .nth(1)
                    .unwrap_or("none")
                    .to_owned();
                let score = by_pv.get(&1).map(|(_, s, _)| *s).unwrap_or(0);
                return Ok((best, score));
            }
            if !line.contains("lowerbound") && !line.contains("upperbound") {
                parse_info_line(&line, &mut by_pv);
            }
        }
    }
}

/// Parse a single `info … multipv … score … pv …` line, updating `by_pv`.
pub fn parse_info_line(line: &str, by_pv: &mut std::collections::HashMap<usize, (String, i32, u32)>) {
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
                    "cp"   => parts[i + 2].parse().ok(),
                    "mate" => parts[i + 2].parse::<i32>().ok().map(|n| {
                        if n > 0 { 100_000 } else { -100_000 }
                    }),
                    _ => None,
                };
                i += 3;
            }
            "pv" if i + 1 < parts.len() => {
                pv_move = Some(parts[i + 1].to_string());
                break;
            }
            _ => { i += 1; }
        }
    }

    if let (Some(d), Some(idx), Some(s), Some(m)) = (line_depth, multipv_idx, score, pv_move) {
        let entry = by_pv.entry(idx).or_insert((String::new(), i32::MIN, 0));
        if d >= entry.2 { *entry = (m, s, d); }
    }
}
