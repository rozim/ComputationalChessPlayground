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

HASH = 4096
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
  engine.configure({'UCI_ShowWDL': 'true'})


  board = chess.Board(FLAGS.fen)
  ##engine.configure({'Clear Hash': None})
  #'{depth:2d}. | {ev:+6d} | {wdl:4.2f} | {n:>12} | {t:6.2f}s | {pv}'
  for clear_hash in [True, False]:
    engine.configure({'Clear Hash': None})
    t1 = time.time()
    print()
    print('# Clear hash: ', clear_hash)
    for depth in range(FLAGS.depth+1):
      multi = engine.analyse(
        board, chess.engine.Limit(depth=depth), multipv=1)
      #pprint.pprint(multi[0])

      m = multi[0]
      wdl = m['wdl'].pov(WHITE).expectation()
      pv = ' '.join(to_san(board, m['pv']))
      ev = simplify_score(m['score'], board)
      nodes = m['nodes']
      t = float(m['time'])

      n = f'{nodes:,}'
      print(f'{depth:2d}. | {ev:+6d} | {wdl:4.2f} | {n:>12} | {t:6.2f}s | {pv}')
      if clear_hash:
        engine.configure({'Clear Hash': None})
    t2 = time.time()
    print('# Clear hash: ', clear_hash, f'time: {t2-t1:.1f}s')

  print('')
  print('Reverse')
  engine.configure({'Clear Hash': None})
  for depth in range(FLAGS.depth, 0, -1):
    multi = engine.analyse(
      board, chess.engine.Limit(depth=depth), multipv=1)

    m = multi[0]
    wdl = m['wdl'].pov(WHITE).expectation()
    pv = ' '.join(to_san(board, m['pv']))
    ev = simplify_score(m['score'], board)
    nodes = m['nodes']
    t = float(m['time'])

    n = f'{nodes:,}'
    print(f'{depth:2d}. | {ev:+6d} | {wdl:4.2f} | {n:>12} | {t:6.2f}s | {pv}')
  print(f'# Reverse, time: {t2-t1:.1f}s')


  engine.quit()

if __name__ == "__main__":
  app.run(main)
