# Print out WDL stats on all moves.

import chess
from chess import WHITE, BLACK
import chess.engine
import chess.pgn
import pprint
import sys
import os
import time


from absl import app
from absl import flags

HASH = 1024
THREADS = 1

FLAGS = flags.FLAGS
flags.DEFINE_string('engine', './stockfish', '')
flags.DEFINE_string('fen', 'r1bq1b1r/pppnpkpp/8/3n4/3P4/8/PPP2PPP/RNBQKB1R w KQ - 0 1', '')
flags.DEFINE_integer('depth', 20, '')

def simplify_pv(pv):
  return [move.uci() for move in pv]


def simplify_score(score, board):
  return score.pov(WHITE).score()


def to_san(board, pv):
  board = board.copy()
  res = []
  for move in pv:
    res.append(board.san(move))
    board.push(move)
  return res


def main(_argv):
  assert os.path.exists(FLAGS.engine), FLAGS.engine
  engine = chess.engine.SimpleEngine.popen_uci(FLAGS.engine)

  engine.configure({'Hash': HASH})
  engine.configure({'Threads': THREADS})
  engine.configure({'Hash': HASH})
  engine.configure({'UCI_ShowWDL': 'true'})

  engine.configure({"Clear Hash": None})


  board = chess.Board(FLAGS.fen)


  engine.configure({"Clear Hash": None})

  print('#####')
  print()
  for d in range(FLAGS.depth):
    multi = engine.analyse(
      board, chess.engine.Limit(depth=d), multipv=1)
    m = multi[0]
    wdl = m['wdl'].pov(WHITE).expectation()
    pv_list = to_san(board, m['pv'])
    pv = ' '.join(pv_list)
    ev = simplify_score(m['score'], board)
    nodes = m['nodes']
    t = m['time']
    print(f'{d:2d} | {ev:10d} | {pv_list[0]:8s} | {wdl:6.3f} | {nodes:10d} | {t:6.1f}s | {pv}')
    sys.stdout.flush()

  print()
  print('#####')
  print()
  engine.configure({"Clear Hash": None})
  multi = engine.analyse(
    board, chess.engine.Limit(depth=FLAGS.depth), multipv=99)
  for i, m in enumerate(multi):
    wdl = m['wdl'].pov(WHITE).expectation()
    pv_list = to_san(board, m['pv'])
    pv = ' '.join(pv_list)
    ev = simplify_score(m['score'], board)
    print(f'{i:2d} {pv_list[0]:8s} {wdl:6.3f} {ev:10d} {pv}')

  print()


  engine.quit()

  #w = multi[0]["wdl"]
  #print(w.white(), w.black())
  #print(w.white().expectation(), w.black().expectation())


if __name__ == "__main__":
  app.run(main)
