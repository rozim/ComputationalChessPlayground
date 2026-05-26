# Generate training data with lc0/leelachesszero.
#
# This may be an idea from 'The Silicon Road to Chess Improvement'
# by GM Matthew Sadler.
#
# Use WDL.
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

import numpy as np


FLAGS = flags.FLAGS
flags.DEFINE_integer('max_ply', 100, '')
flags.DEFINE_integer('num_games', 1, 'Number of games to generate. Note that Lichess studys have a 32 game limit.')
flags.DEFINE_string('fen', 'rnbqkb1r/pppppppp/8/6B1/3Pn2P/8/PPP1PPP1/RN1QKBNR b KQkq - 0 3', '')

flags.DEFINE_float('time', 30.0, 'Time in seconds for entire game')
flags.DEFINE_float('inc', 1.0, 'Increment in seconds')

flags.DEFINE_integer('multipv', 3, '')

LC0 = 'lc0'

T_INIT = 0.01 # Initial value, should heavily favor first entry.
T_MUL  = 2.0


def raw_position_fen(board):
  #rn2kbnr/ppq2pp1/4p3/2pp2Bp/2P4P/1Q6/P2NNPP1/3RK2R w Kkq - 2 13
  return ' '.join(board.fen().split(' ')[0])


def softmax(x, temperature=1.0):
  x_max = np.max(x)  # Avoid overflow
  e_x = np.exp((x - x_max) / temperature)
  return e_x / np.sum(e_x)



def show(xx):
  ar = [f'{x:.2f}' for x in xx]
  return ' '.join(ar)


class MovePicker:
  def __init__(self):
    self.rng = np.random.default_rng()

  def pick_move(self, multi, temperature=0.1, log=None):
    if multi[0]['score'].white().is_mate():
      if log:
        log.write('\tmate')
      return multi[0]['pv'][0]

    if len(multi) == 1:
      if log:
        log.write('\tforced')
      return multi[0]['pv'][0]

    a = [m['pv'][0] for m in multi]
    x = [m['wdl'].relative.expectation() for m in multi]
    p = softmax(x, temperature)
    res = self.rng.choice(a=a, p=p)
    if log:
      log.write(f'\tt={temperature:.2f} a={[_.uci() for _ in a]} x={x} p={p} index={a.index(res)}\n')
    return res



def play_game(engine, starting_fen, already, picker, log, freqs):
  log.write('\n')

  board = chess.Board(starting_fen)
  ply = -1
  remaining_time = [FLAGS.time, FLAGS.time]
  novelty = None
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
    raw = raw_position_fen(board)

    if raw in already:  # Already visited, use temperature.
      temperature = freqs.get(raw, T_INIT)
      move = picker.pick_move(multi, temperature=temperature, log=log)
      freqs[raw] = temperature * T_MUL
    else:  # First time in a node play best.
      move = multi[0]['pv'][0]

    dt = time.time() - t1

    remaining_time[board.turn] -= dt
    remaining_time[board.turn] += FLAGS.inc

    log.write(f'{ply} dt={dt:.2f} rt={remaining_time[board.turn]:.2f} {move.uci()}\n')
    log.flush()


    if raw not in already:
      if novelty is None:
        novelty = ply
      already.add(raw)

    board.push(move)
  return board, novelty


def generate_game(board, elapsed, starting_fen, xround):
  game = chess.pgn.Game()
  game.setup(starting_fen)
  game.headers['Event'] = 'Generate game'
  game.headers['Date'] = datetime.date.today().strftime('%Y.%m.%d')
  game.headers['White'] = 'lc0'
  game.headers['Black'] = 'lc0'
  game.headers['Round'] = str(xround)
  outcome = board.outcome()
  if outcome:
    game.headers['Result'] = outcome.result()
  else:
    game.headers['Result'] = '1/2 - 1/2'

  node = game
  for move in board.move_stack:
    node = node.add_main_variation(move)
  return game


def main(argv):

  picker = MovePicker()
  engine = chess.engine.SimpleEngine.popen_uci(LC0)
  engine.configure({'UCI_ShowWDL': True})
  f_pgn = open(f'games-{int(time.time())}.pgn', 'w')
  already = set()
  freqs = {} # fen -> #

  log = open('log.txt', 'w')

  for n in range(FLAGS.num_games):
    t1 = time.time()
    final_board, novelty = play_game(engine, FLAGS.fen, already, picker, log, freqs)
    dt = time.time() - t1
    game = generate_game(final_board, dt, FLAGS.fen, n + 1)
    print(f'Game {n} {dt:.1f}s ply={len(final_board.move_stack)} {final_board.outcome()} novelty={novelty}')
    log.write(f'Game {n} {dt:.1f}s ply={len(final_board.move_stack)} {final_board.outcome()} novelty={novelty}\n')

    f_pgn.write(game.accept(chess.pgn.StringExporter(columns=75)) + '\n\n')
    f_pgn.flush()

  f_pgn.close()
  engine.quit()



if __name__ == '__main__':
  app.run(main)
