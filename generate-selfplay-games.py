# Generate training data with stockfish.
# Start off in common ECO positions.
#
# Usually make the best move but sometimes make
# a random move.
#
# Limit the max ply so that random moves by the stonger
# side don't prolong games forever.
#

import chess
import chess.engine
import sys, os
import random
import time
from random import random, choice

from absl import app
from absl import flags
from absl import logging


FLAGS = flags.FLAGS
flags.DEFINE_integer('goal', 10, '')
flags.DEFINE_integer('depth', 1, 'search depth')
flags.DEFINE_integer('max_game_ply', 100, '')

STOCKFISH = './stockfish'
HASH = 512
THREADS =1
PCT_RANDOM = 0.25

max_game = 0


def parse_eco_fen():
  for what in ['a', 'b', 'c', 'd', 'e']:
    with open(f'eco/{what}.tsv', 'r') as f:
      first = True
      for line in f:
        if first:
          first = False
          continue
        yield line.split('\t')[2]


def read_eco():
  ecos = list(parse_eco_fen())
  ecos.append('rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1')
  return ecos



def play2(engine, starting_fen, pct_random):
  board = chess.Board(starting_fen)
  ply = -1
  while board.outcome() is None:
    ply += 1
    if ply >= FLAGS.max_game_ply:
      global max_game
      max_game += 1
      return

    # Make every move, even complete garbage, and analyze, so that
    # the ML model learns to make obvious moves/recaptures etc.
    for move in board.legal_moves:
      board.push(move)
      if board.outcome() is None:
        res = engine.analyse(board, chess.engine.Limit(depth=FLAGS.depth))
        yield simplify_fen(board), res['pv'][0], simplify_score2(res['score'])[-1]
      board.pop()

    if random() < pct_random:
      # To add variety, sometimes just move randomly and
      # don't analyze.
      move = choice(list(board.legal_moves))
    else: # Play best
      engine.configure({"Clear Hash": None})
      res = engine.analyse(board, chess.engine.Limit(depth=FLAGS.depth))
      move = res['pv'][0]
    board.push(move)


def simplify_fen(board):
  #rn2kbnr/ppq2pp1/4p3/2pp2Bp/2P4P/1Q6/P2NNPP1/3RK2R w Kkq - 2 13
  return ' '.join(board.fen().split(' ')[0:4])


def simplify_score2(score):
  mx = 10000
  lim = 9000
  res = int(score.pov(chess.WHITE).score(mate_score=10000))
  if score.is_mate(): # normal, mate
    assert res > lim or res < -lim, 'mate in 1000 considered unlikely'
    return True, res
  elif res > lim: # clamp
    return False, lim
  elif res < -lim: # clamp
    return False, -lim
  else: # normal, in range
    return False, res


def main(argv):
  ecos = read_eco()

  engine = chess.engine.SimpleEngine.popen_uci(STOCKFISH)
  engine.configure({"Hash": HASH})
  engine.configure({"Threads": THREADS})

  all_fens = set()
  dups = 0
  games = 0
  go_on = True

  while go_on:
    games += 1
    for fen, move, score in play2(engine, choice(ecos), pct_random=PCT_RANDOM):
      if fen in all_fens:
        dups += 1
        continue
      all_fens.add(fen)
      print(fen, move, score)
      if len(all_fens) >= FLAGS.goal:
        go_on = False
        break

  engine.quit()

  global max_game
  print('fens: ', len(all_fens), 'dups', dups, 'games', games, 'max games', max_game)


if __name__ == '__main__':
  app.run(main)
