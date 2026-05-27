use shakmaty::{Chess, Color, KnownOutcome, Outcome, Position, uci::UciMove, san::San};
use stockfish::Stockfish;

const STOCKFISH_PATH: &str = "/usr/local/bin/stockfish";
const DEPTH: u32 = 5; // shallow depth so the game runs quickly

fn main() -> std::io::Result<()> {
    let mut engine = Stockfish::new(STOCKFISH_PATH)?;
    engine.set_depth(DEPTH);
    engine.setup_for_new_game()?;
    engine.reset_position()?;

    let mut pos = Chess::default();
    let mut move_number = 1;

    println!("Stockfish {} vs Stockfish {} (depth {DEPTH})",
        engine.get_version().as_deref().unwrap_or("?"),
        engine.get_version().as_deref().unwrap_or("?"));
    println!();

    loop {
        // Ask Stockfish for the best move given the current position.
        let output = engine.go()?;
        let uci_str = output.best_move();

        // "none" means the engine sees no legal moves (game already over).
        if uci_str == "none" {
            break;
        }

        // Parse the UCI move string and resolve it against the current position.
        let uci_move: UciMove = uci_str.parse().unwrap_or_else(|e| {
            panic!("Could not parse UCI move '{uci_str}': {e}");
        });
        let m = uci_move.to_move(&pos).unwrap_or_else(|e| {
            panic!("Illegal move '{uci_str}': {e}");
        });

        // Format as SAN before playing (SAN needs the pre-move position).
        let san = San::from_move(&pos, m.clone());

        // Print move in standard score-sheet style.
        if pos.turn() == Color::White {
            print!("{move_number}. {san}");
        } else {
            println!("  {san}");
            move_number += 1;
        }

        // Play on shakmaty board.
        pos.play_unchecked(m);

        // Inform the engine so it tracks the same position.
        engine.play_move(uci_str)?;

        // Check for game over.
        if pos.is_game_over() {
            break;
        }
    }

    // Ensure we don't leave a dangling half-line if game ended on White's move.
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

    engine.quit()?;
    Ok(())
}
