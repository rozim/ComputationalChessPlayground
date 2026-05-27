use std::env;

use pgn_game_reader::read_games;

fn main() {
    let path = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: game-reader-demo <file.pgn>");
        std::process::exit(1);
    });

    let games = match read_games(&path) {
        Ok(g) if g.is_empty() => {
            eprintln!("No games found in {path}");
            std::process::exit(1);
        }
        Ok(g) => g,
        Err(e) => {
            eprintln!("Error reading {path}: {e}");
            std::process::exit(1);
        }
    };

    let game = &games[0];

    // ── Tags ──────────────────────────────────────────────────────────────────
    println!("=== Tags ({}) ===", game.tags.len());
    let mut tags: Vec<(&String, &String)> = game.tags.iter().collect();
    tags.sort_by_key(|(k, _)| k.as_str());
    for (key, value) in &tags {
        println!("  {key}: {value}");
    }

    // ── Outcome ───────────────────────────────────────────────────────────────
    println!("\n=== Outcome ===");
    println!("  {}", game.outcome);

    // ── Moves ─────────────────────────────────────────────────────────────────
    println!("\n=== Moves ({}) ===", game.moves.len());
    for (i, chunk) in game.moves.chunks(2).enumerate() {
        let move_num = i + 1;
        match chunk {
            [white, black] => println!("  {move_num:3}. {white:<8} {black}"),
            [white]        => println!("  {move_num:3}. {white}"),
            _              => {}
        }
    }
}
