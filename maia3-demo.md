# Goal

Create a Rust tool that runs the maia3 chess engine on a PGN file.

## Guidelines

search with UCI command "go nodes 1"

ELO is from the list 1200, 1600, 1800, 2000, 2200, 2400, 2600, 2800

To query the maia3 chess engine iwth a given ELO setting use the UCI option 'Elo'.

Try to use the Rust creates: shakmaty, stockfish, pgn-reader

Set MultiPV to 1.

Print moves in SAN.

In the output, if a move for a given ELO is the same as the move for the previous ELO, then don't print
it, just print a single ".", to keep the output cleaner.

## Command line arguments

optional flag "--min_ply=N" - to skip this many ply at the start of the game before
calling maia3 - default this value to 10

positional / last arg: PGN file

## Logic

for every PGN file
{
  for every chess game in the PGN file
  {
    for every move in the game
    {
      for every ELO from the list {
        invoke maia3 with the specified ELO to find the best move
	record if the move matches the played move
    }
    at end of move emit move played and predictions for each ELO
   }
   at end of game print accuracy for each ELO
}


# References

maia3 home page : https://github.com/CSSLab/maia3
Paper on maia3 titled Chessformer: A Unified Architecture for Chess Modeling : https://arxiv.org/abs/2605.19091 :
maia3 binary chess engine that runs UCI protocol: /Users/dave/venv-maia3/bin/maia3-23m
