# Generate training data with stockfish.
# Start off in common ECO positions.
#
# Usually make the best move but sometimes make
# a random move.
#
# Limit the max ply so that random moves by the stonger
# side don't prolong games forever.
#

import datetime
import sys, os
import time
from random import random, choice

import chess
import chess.pgn
from chess import WHITE, BLACK
import chess.engine

from absl import app
from absl import flags
from absl import logging


FLAGS = flags.FLAGS
flags.DEFINE_integer('max_ply', 100, '')
flags.DEFINE_integer('num_games', 1, 'Number of games to generate')
flags.DEFINE_integer('hash', 16, '')
flags.DEFINE_integer('threads', 1, '')
flags.DEFINE_string('fen', 'rnbqkb1r/pppppppp/8/6B1/3Pn2P/8/PPP1PPP1/RN1QKBNR b KQkq - 0 3', '')

flags.DEFINE_float('time', 30.0, 'Time in seconds for entire game')
flags.DEFINE_float('inc', 1.0, 'Increment in seconds')

flags.DEFINE_integer('multipv', 1, '')
flags.DEFINE_integer('threshold', 0, 'score delta')
flags.DEFINE_float('pct', 0.25, 'How often to choose a non-optimal move with the threshold')

STOCKFISH = './stockfish'
PCT_RANDOM = 0.25

def play_game(engine, starting_fen):
  board = chess.Board(starting_fen)
  ply = -1
  remaining_time = [FLAGS.time, FLAGS.time]
  while board.outcome() is None:
    ply += 1
    if FLAGS.max_ply and ply >= FLAGS.max_ply:
      break

    t1 = time.time()
    multi = engine.analyse(board, chess.engine.Limit(white_clock=remaining_time[WHITE],
                                                   black_clock=remaining_time[BLACK],
                                                   white_inc=FLAGS.inc,
                                                   black_inc=FLAGS.inc),
                         multipv=FLAGS.multipv)
    scores = [m['score'].white().score(mate_score=10000) for m in multi]

    alt = []
    for i in range(1, len(scores)):
      if abs(scores[i] - scores[0]) < FLAGS.threshold:
        alt.append(multi[i]['pv'][0])

    if not multi[0]['score'].white().is_mate() and len(alt) > 0 and random() < FLAGS.pct:
      move = choice(alt)
    else:
      move = multi[0]['pv'][0]
    dt = time.time() - t1

    remaining_time[board.turn] -= dt
    remaining_time[board.turn] += FLAGS.inc
    # print(f'MOVE: {move}, {remaining_time[WHITE]:.1f} {remaining_time[BLACK]:.1f} {dt:.1f} {scores} {alt} {board.fen()}')

    board.push(move)
  return board


def generate_game(board, elapsed, starting_fen, xround):
  game = chess.pgn.Game()
  game.setup(starting_fen)
  game.headers['Event'] = 'Generate game'
  game.headers['Date'] = datetime.date.today().strftime('%Y.%m.%d')
  game.headers['White'] = 'Stockfish'
  game.headers['Black'] = 'Stockfish'
  game.headers['Round'] = str(xround)
  outcome = board.outcome()
  if outcome:
    game.headers['Result'] = outcome.result()
  else:
    game.headers['Result'] = '1/2 - 1/2'
  game.headers['X-Duration'] = f'{elapsed:.1f}s'
  game.headers['X-Time-Sec'] = str(FLAGS.time)
  game.headers['X-Inc-Sec'] = str(FLAGS.inc)
  game.headers['X-Threshold'] = str(FLAGS.threshold)

  node = game
  for move in board.move_stack:
    node = node.add_main_variation(move)
  return game


def main(argv):

  engine = chess.engine.SimpleEngine.popen_uci(STOCKFISH)
  engine.configure({"Hash": FLAGS.hash})
  engine.configure({"Threads": FLAGS.threads})
  f_pgn = open('games.pgn', 'w')

  for n in range(FLAGS.num_games):
    t1 = time.time()
    final_board = play_game(engine, FLAGS.fen)
    dt = time.time() - t1
    game = generate_game(final_board, dt, FLAGS.fen, n + 1)
    print(f'Game {n} {dt:.1f}s ply={len(final_board.move_stack)} {final_board.outcome()}')
    # print(str(game))

    f_pgn.write(str(game) + '\n\n')
    f_pgn.flush()

  f_pgn.close()
  engine.quit()



if __name__ == '__main__':
  app.run(main)
