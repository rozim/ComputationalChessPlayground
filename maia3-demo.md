# Goal

Create a Rust tool that runs the maia3 chess engine and Stockfish chess engine on a PGN file.

## Guidelines

With maia3 search with UCI command "go nodes 1".
With stockfish search with the indicated nodes value.

For maia3 ELO is from the list 1200, 1600, 1800, 2000, 2200, 2400, 2600, 2800
For stockfish use these nodes and these names for the nodes:
  * nodes=1_000 name=1k
  * nodes=10_000 name=10k
  * nodes=100_000 name=100k
  * nodes=1_000_000 name=1M

To query the maia3 chess engine with a given ELO setting use the UCI option 'Elo'.

Try to use the Rust creates: shakmaty, stockfish, pgn-reader

Set MultiPV to 1.

Print moves in SAN.

For the maia3 part of the output:
* In the output, if a move for a given ELO is the same as the move for the previous ELO, then don't print
it, just print a single ".", to keep the output cleaner.

For the stockfish part of the output:
* use the name of the nodes used e.g. "100k"
* If the move is the same as in the previous nodes column then print a "."

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
      for every node setting from the number of nodes map (with name and nodes):
        invoke stockfish with the specified number of nodes
	record if the move matches the played move, store by players color
    }
    at end of move emit move played and predictions for each ELO (maia3) and for each nodes value (stockfish)
   }
   for each players color: at end of game print accuracy for each ELO and nodes name/value
}


# References

maia3 home page : https://github.com/CSSLab/maia3
Paper on maia3 titled Chessformer: A Unified Architecture for Chess Modeling : https://arxiv.org/abs/2605.19091 :
maia3 binary chess engine that runs UCI protocol: /Users/dave/venv-maia3/bin/maia3-23m
stockfish binary: /usr/local/bin/stockfish
