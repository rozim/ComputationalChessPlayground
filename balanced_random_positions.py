import chess
from chess import WHITE, BLACK
import chess.engine
import chess.pgn
import pprint
import sys
import os
import time
import pprint
import random


from absl import app
from absl import flags

HASH = 4096
THREADS = 1

FLAGS = flags.FLAGS
flags.DEFINE_string('engine', './stockfish', '')
flags.DEFINE_integer('opening_depth', 10, '')
flags.DEFINE_integer('search_depth', 6, '')
flags.DEFINE_integer('alternatives', 2, '')
flags.DEFINE_integer('alternative_window', 50, '')
flags.DEFINE_integer('goal', 10, '')
flags.DEFINE_integer('position_max_score', 100, '')

best = 900 # global



def go_down(board, remain, already, engine):
    fen = board.fen()
    if fen in already:
        return
    already.add(fen)

    res = engine.analyse(board, chess.engine.Limit(depth=1))
    white = res['score'].white()
    if white.is_mate():
        return
    if abs(white.score()) >= FLAGS.position_max_score:
        return # Intermediate position too imbalanced.

    if remain <= 0:
        multi = engine.analyse(
            board, chess.engine.Limit(depth=FLAGS.search_depth), multipv=FLAGS.alternatives)
        if len(multi) < FLAGS.alternatives:
            return
        white = multi[0]['score'].white()
        if white.is_mate():
            return
        white1 = multi[FLAGS.alternatives - 1]['score'].white()
        if white1.is_mate():
            return
        if abs(white1.score()) >= FLAGS.position_max_score:
            return # Position too unbalanced
        if abs(white.score()) >= FLAGS.position_max_score:
            return # Position too unbalanced

        if abs((white.score() - white1.score()) <= FLAGS.alternative_window):
            foo = [board.san(m['pv'][0]) for m in multi]
            yield board.fen(), board.move_stack, white, ' '.join(foo)
        return

    legal = list(board.legal_moves)
    random.shuffle(legal)
    for m in legal:
        board.push(m)
        ok = (board.outcome() is None)
        if ok:
            yield from go_down(board, remain - 1, already, engine)
        board.pop()



def main(_argv):
    global best
    assert os.path.exists(FLAGS.engine), FLAGS.engine

    engine = chess.engine.SimpleEngine.popen_uci(FLAGS.engine)

    engine.configure({'Hash': HASH})
    engine.configure({'Threads': THREADS})
    engine.configure({'UCI_ShowWDL': 'true'})

    for _ in range(FLAGS.goal):
        fen, move_stack, white, foo = next(go_down(chess.Board(), FLAGS.opening_depth, set(), engine))
        sans = []
        tmp = chess.Board()
        for ms in move_stack:
            sans.append(tmp.san(ms))
            tmp.push(ms)

        print(white.score(), '|', fen, '|', ' '.join(sans))
        print('\t', foo)

    engine.quit()


if __name__ == "__main__":
  app.run(main)
