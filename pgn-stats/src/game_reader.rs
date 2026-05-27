use std::env;

use pgn_game_reader::read_games;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        eprintln!("Usage: pgn-stats-game-reader <file.pgn> [file.pgn ...]");
        std::process::exit(1);
    }

    let mut total_games = 0usize;
    let mut total_moves = 0usize;

    for path in &args {
        match read_games(path) {
            Ok(games) => {
                for game in &games {
                    total_moves += game.moves.len();
                }
                total_games += games.len();
            }
            Err(e) => eprintln!("Could not read {path}: {e}"),
        }
    }

    println!("Games: {total_games}");
    println!("Moves: {total_moves}");
}
