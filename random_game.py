import chess
import random
import collections
import pprint
import tqdm

board = chess.Board()
while board.outcome() is None:
  move = random.choice(list(board.legal_moves))
  print(board.uci(move))
  board.push(move)
outcome = board.outcome()
print('OUTCOME: ', outcome)
print('WINNER: ', outcome.winner)
print('RESULT: ', outcome.result())
print('TERMINATION: ', outcome.termination)
print('FEN: ', board.fen())

print()
print('10000 random')
outcomes = collections.Counter()
results = collections.Counter()
for _ in tqdm.trange(10000):
  board = chess.Board()
  while board.outcome() is None:
    move = random.choice(list(board.legal_moves))
    board.push(move)
  outcome = board.outcome()
  s = str(outcome) + " " + outcome.result()
  results[outcome.result()] += 1
  outcomes[s] += 1

pprint.pprint(outcomes, width=1)
print()
pprint.pprint(results, width=1)
