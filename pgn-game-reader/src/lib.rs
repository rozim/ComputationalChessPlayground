use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufReader},
    ops::ControlFlow,
    path::Path,
};

use pgn_reader::{Outcome, RawTag, Reader, SanPlus, Visitor};

/// The outcome of a chess game.
pub use pgn_reader::Outcome as GameOutcome;

/// A single parsed chess game.
#[derive(Debug, Clone)]
pub struct Game {
    /// The game result (`1-0`, `0-1`, `1/2-1/2`, or `*` for unknown).
    pub outcome: Outcome,
    /// All PGN tag pairs (e.g. `"White" => "Stockfish"`).
    pub tags: HashMap<String, String>,
    /// Mainline moves in SAN notation (e.g. `["e4", "e5", "Nf3", ...]`).
    pub moves: Vec<String>,
}

// ── Visitor state ─────────────────────────────────────────────────────────────

struct GameVisitor;

/// Accumulated state during the tag phase.
struct TagState {
    tags: HashMap<String, String>,
}

/// Accumulated state during the movetext phase.
struct MovetextState {
    tags: HashMap<String, String>,
    moves: Vec<String>,
    outcome: Outcome,
}

impl Visitor for GameVisitor {
    type Tags = TagState;
    type Movetext = MovetextState;
    type Output = Game;

    fn begin_tags(&mut self) -> ControlFlow<Self::Output, Self::Tags> {
        ControlFlow::Continue(TagState {
            tags: HashMap::new(),
        })
    }

    fn tag(
        &mut self,
        tags: &mut Self::Tags,
        name: &[u8],
        value: RawTag<'_>,
    ) -> ControlFlow<Self::Output> {
        let key = String::from_utf8_lossy(name).into_owned();
        let val = String::from_utf8_lossy(value.as_bytes()).into_owned();
        tags.tags.insert(key, val);
        ControlFlow::Continue(())
    }

    fn begin_movetext(
        &mut self,
        tags: Self::Tags,
    ) -> ControlFlow<Self::Output, Self::Movetext> {
        ControlFlow::Continue(MovetextState {
            tags: tags.tags,
            moves: Vec::new(),
            outcome: Outcome::Unknown,
        })
    }

    fn san(
        &mut self,
        movetext: &mut Self::Movetext,
        san_plus: SanPlus,
    ) -> ControlFlow<Self::Output> {
        movetext.moves.push(san_plus.san.to_string());
        ControlFlow::Continue(())
    }

    fn outcome(
        &mut self,
        movetext: &mut Self::Movetext,
        outcome: Outcome,
    ) -> ControlFlow<Self::Output> {
        movetext.outcome = outcome;
        ControlFlow::Continue(())
    }

    fn end_game(&mut self, movetext: Self::Movetext) -> Self::Output {
        Game {
            outcome: movetext.outcome,
            tags: movetext.tags,
            moves: movetext.moves,
        }
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Read all games from a PGN file, returning one [`Game`] per game.
pub fn read_games<P: AsRef<Path>>(path: P) -> io::Result<Vec<Game>> {
    let file = File::open(path)?;
    let mut reader = Reader::new(BufReader::new(file));
    let mut visitor = GameVisitor;
    let mut games = Vec::new();

    while let Some(game) = reader.read_game(&mut visitor)? {
        games.push(game);
    }

    Ok(games)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use pgn_reader::{KnownOutcome, Outcome};

    const PGN: &[u8] = b"\
[Event \"Test\"]\n\
[White \"Alice\"]\n\
[Black \"Bob\"]\n\
[Result \"1-0\"]\n\
\n\
1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 1-0\n\
\n\
[Event \"Draw\"]\n\
[White \"Carol\"]\n\
[Black \"Dave\"]\n\
[Result \"1/2-1/2\"]\n\
\n\
1. d4 d5 1/2-1/2\n";

    fn parse(pgn: &[u8]) -> Vec<Game> {
        let mut reader = Reader::new(std::io::Cursor::new(pgn));
        let mut visitor = GameVisitor;
        let mut games = Vec::new();
        while let Some(g) = reader.read_game(&mut visitor).unwrap() {
            games.push(g);
        }
        games
    }

    #[test]
    fn game_count() {
        assert_eq!(parse(PGN).len(), 2);
    }

    #[test]
    fn tags() {
        let games = parse(PGN);
        assert_eq!(games[0].tags["White"], "Alice");
        assert_eq!(games[0].tags["Black"], "Bob");
        assert_eq!(games[1].tags["White"], "Carol");
    }

    #[test]
    fn moves() {
        let games = parse(PGN);
        assert_eq!(games[0].moves, ["e4", "e5", "Nf3", "Nc6", "Bb5", "a6"]);
        assert_eq!(games[1].moves, ["d4", "d5"]);
    }

    #[test]
    fn outcome() {
        let games = parse(PGN);
        assert!(matches!(
            games[0].outcome,
            Outcome::Known(KnownOutcome::Decisive { .. })
        ));
        assert!(matches!(
            games[1].outcome,
            Outcome::Known(KnownOutcome::Draw)
        ));
    }
}
