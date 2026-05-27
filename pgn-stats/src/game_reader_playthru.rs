use std::env;

use pgn_game_reader::read_games;
use shakmaty::{CastlingMode, Chess, Position, fen::Fen, san::San};

fn main() {
    let path = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: game-reader-playthru <file.pgn>");
        std::process::exit(1);
    });

    let games = match read_games(&path) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("Error reading {path}: {e}");
            std::process::exit(1);
        }
    };

    let mut total_games = 0usize;
    let mut total_moves = 0usize;
    let mut error_count = 0usize;

    for (game_idx, game) in games.iter().enumerate() {
        // Build starting position: custom FEN if present, else standard start.
        let mut pos: Chess = match game.tags.get("FEN") {
            Some(fen_str) => {
                let fen: Fen = match fen_str.parse() {
                    Ok(f) => f,
                    Err(e) => {
                        eprintln!("Game {}: bad FEN \"{fen_str}\": {e}", game_idx + 1);
                        error_count += 1;
                        continue;
                    }
                };
                match fen.into_position(CastlingMode::Standard) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("Game {}: invalid position \"{fen_str}\": {e}", game_idx + 1);
                        error_count += 1;
                        continue;
                    }
                }
            }
            None => Chess::default(),
        };

        // Play through every move.
        let mut move_error = false;
        for (move_idx, san_str) in game.moves.iter().enumerate() {
            let san: San = match san_str.parse() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!(
                        "Game {}, move {}: could not parse \"{san_str}\": {e}",
                        game_idx + 1,
                        move_idx + 1
                    );
                    move_error = true;
                    break;
                }
            };

            let m = match san.to_move(&pos) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!(
                        "Game {}, move {}: illegal \"{san_str}\": {e}",
                        game_idx + 1,
                        move_idx + 1
                    );
                    move_error = true;
                    break;
                }
            };

            pos.play_unchecked(m);
            total_moves += 1;
        }

        if move_error {
            error_count += 1;
        } else {
            total_games += 1;
        }
    }

    println!("Games played through: {total_games}");
    println!("Moves played:         {total_moves}");
    if error_count > 0 {
        println!("Errors:               {error_count}");
    }
}
