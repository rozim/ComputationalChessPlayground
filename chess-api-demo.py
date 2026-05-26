import chess
import chess.pgn
from chess import WHITE, BLACK
import chess.engine
import math
import numpy as np
import pprint


FEN = 'rnb1k1nr/pp3ppp/2p1p3/8/1BP1q3/8/PP2BPPP/R2QK1NR b KQkq - 1 8'
FEN = 'rnb1k1nr/1p3ppp/2p1p3/p7/1BP1q3/8/PP2BPPP/R2QK1NR w KQkq - 0 9'
engine = chess.engine.SimpleEngine.popen_uci('lc0')
engine.configure({'UCI_ShowWDL': True})
# engine.configure({'Clear Hash': None})
# engine.configure({'Hash': 256})
# engine.configure({'Threads': 1})


# https://www.chess-journal.com/evaluatingSharpness2.html
# https://twitter.com/LeelaChessZero/status/1637763896596463616
# ugh, typo in their tweet it seems
# def sharpness(wdl):
#   w = wdl.wins
#   l = wdl.losses
#   log = math.log
#   return 2.0 / (log((1.0 / w) - 1) +
#                 log((1.0 / l) - 1))


def softmax(x, temperature=1.0):
  x_max = np.max(x)  # Avoid overflow
  e_x = np.exp((x - x_max) / temperature)
  return e_x / np.sum(e_x)


def to_san(board, pv):
  board = board.copy()
  res = []
  for move in pv:
    res.append(board.san(move))
    board.push(move)
  return res

board = chess.Board(FEN)
multi = engine.analyse(board, chess.engine.Limit(time=10), multipv=10)


print('Nodes', multi[0]['nodes'])
print()
for i, m in enumerate(multi):
  san = to_san(board, m['pv'])
  score = m['score'].pov(WHITE).score(mate_score=10000)
  wdl = m['wdl'].relative

  print(f'{i}. {san[0]:10s} s={score:8d} ex={wdl.expectation():.2f} dr={wdl.draws/1000.0:.2f} w={wdl.wins:4d} d={wdl.draws:4d} l={wdl.losses:4d} pv={" ".join(san)}')

x = [m['wdl'].relative.expectation() for m in multi]
print('x: ', x)
print('softmax: ', [f'{s:.2f}' for s in softmax(x)])
print('softmax: ', [f'{s:.2f}' for s in softmax(x, 0.1)])
print('softmax: ', [f'{s:.2f}' for s in softmax(x, 0.5)])
print('softmax: ', [f'{s:.2f}' for s in softmax(x, 2)])
print('softmax: ', [f'{s:.2f}' for s in softmax(x, 10)])




engine.quit()
