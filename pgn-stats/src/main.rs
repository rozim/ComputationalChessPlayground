use std::{env, fs::File, io::BufReader, ops::ControlFlow};

use pgn_reader::{Reader, SanPlus, Visitor};

struct Counter;

impl Visitor for Counter {
    type Tags = ();
    type Movetext = usize;
    type Output = usize;

    fn begin_tags(&mut self) -> ControlFlow<Self::Output, Self::Tags> {
        ControlFlow::Continue(())
    }

    fn begin_movetext(
        &mut self,
        _tags: Self::Tags,
    ) -> ControlFlow<Self::Output, Self::Movetext> {
        ControlFlow::Continue(0)
    }

    fn san(
        &mut self,
        movetext: &mut Self::Movetext,
        _san_plus: SanPlus,
    ) -> ControlFlow<Self::Output> {
        *movetext += 1;
        ControlFlow::Continue(())
    }

    fn end_game(&mut self, movetext: Self::Movetext) -> Self::Output {
        movetext
    }
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        eprintln!("Usage: pgn-stats <file.pgn> [file.pgn ...]");
        std::process::exit(1);
    }

    let mut total_games = 0usize;
    let mut total_moves = 0usize;

    for path in &args {
        let file = File::open(path)?;
        let mut reader = Reader::new(BufReader::new(file));
        let mut visitor = Counter;

        while let Some(moves) = reader.read_game(&mut visitor)? {
            total_games += 1;
            total_moves += moves;
        }
    }

    println!("Games: {total_games}");
    println!("Moves: {total_moves}");

    Ok(())
}
