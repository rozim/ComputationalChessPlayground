//! Move and game accuracy computation, ported from lichess-org/lila.
//!
//! Reference:
//!   <https://github.com/lichess-org/lila/blob/2e653ad1e2b9fad31b4a092394019ef8fafdedb8/modules/analyse/src/main/AccuracyPercent.scala>
//!   <https://github.com/lichess-org/scalachess/blob/master/core/src/main/scala/eval.scala>

// ── Win-percent conversion ────────────────────────────────────────────────────

/// Centipawn evaluation of the starting position (mirrors `Cp.initial = 15`).
pub const INITIAL_CP: i32 = 15;

/// Maximum centipawn value before clamping (mirrors `Cp.CEILING = 1000`).
const CP_CEILING: i32 = 1000;

/// Convert a centipawn evaluation (from white's perspective) to a win
/// percentage in [0, 100].
///
/// Mirrors `WinPercent.fromCentiPawns`:
///   - Clamp cp to `[-CP_CEILING, CP_CEILING]`
///   - `50 + 50 * (2 / (1 + exp(-0.00368208 * cp)) - 1)`
pub fn win_percent_from_cp(cp: i32) -> f64 {
    let cp = cp.clamp(-CP_CEILING, CP_CEILING) as f64;
    let winning_chances = (2.0 / (1.0 + f64::exp(-0.00368208 * cp)) - 1.0).clamp(-1.0, 1.0);
    50.0 + 50.0 * winning_chances
}

// ── Move accuracy ─────────────────────────────────────────────────────────────

/// Accuracy of a single move in [0, 100], given the win percentage **for the
/// moving side** before and after the move.
///
/// Mirrors `AccuracyPercent.fromWinPercents`:
///   - If the position improved (`after >= before`), accuracy is 100.
///   - Otherwise it falls off exponentially with the win-percent loss.
pub fn accuracy_from_win_percents(before: f64, after: f64) -> f64 {
    if after >= before {
        100.0
    } else {
        let win_diff = before - after;
        let raw = 103.1668100711649 * f64::exp(-0.04354415386753951 * win_diff)
            - 3.166924740191411;
        (raw + 1.0).clamp(0.0, 100.0)
    }
}

// ── Statistics helpers ────────────────────────────────────────────────────────

/// Population standard deviation. Returns `None` for an empty slice.
fn std_dev(xs: &[f64]) -> Option<f64> {
    let n = xs.len();
    if n == 0 {
        return None;
    }
    let mean = xs.iter().sum::<f64>() / n as f64;
    let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64;
    Some(var.sqrt())
}

/// Weighted mean of `(value, weight)` pairs. Returns `None` for empty input or
/// zero total weight.
fn weighted_mean(pairs: &[(f64, f64)]) -> Option<f64> {
    if pairs.is_empty() {
        return None;
    }
    let sum_w: f64 = pairs.iter().map(|(_, w)| w).sum();
    if sum_w == 0.0 {
        return None;
    }
    Some(pairs.iter().map(|(v, w)| v * w).sum::<f64>() / sum_w)
}

/// Harmonic mean. Returns `None` for an empty slice.
fn harmonic_mean(xs: &[f64]) -> Option<f64> {
    if xs.is_empty() {
        return None;
    }
    let sum_recip: f64 = xs.iter().map(|x| 1.0 / x).sum();
    if sum_recip == 0.0 {
        return None;
    }
    Some(xs.len() as f64 / sum_recip)
}

// ── Game accuracy ─────────────────────────────────────────────────────────────

/// Compute game accuracy for both players from a sequence of centipawn scores.
///
/// `start_color_is_white` — `true` when white made the first move (the normal
/// case).
///
/// `initial_cp` — engine evaluation (from white's perspective, in centipawns)
/// of the position **before** the first move in `cps`.  Use [`INITIAL_CP`] for
/// a standard game starting from move 1; use the actual Stockfish score when
/// `--min-ply` skips the opening.
///
/// `cps` — engine evaluation **after each half-move**, from white's
/// perspective, in centipawns.  One entry per ply.
///
/// Returns `Some((white_accuracy, black_accuracy))`, each in [0, 100], or
/// `None` if there is insufficient data (fewer than two positions).
///
/// Algorithm: direct port of `AccuracyPercent.gameAccuracy` from lila.
pub fn game_accuracy(start_color_is_white: bool, initial_cp: i32, cps: &[i32]) -> Option<(f64, f64)> {
    // Prepend the initial-position evaluation, then convert to win percents.
    let all_wp: Vec<f64> = std::iter::once(initial_cp)
        .chain(cps.iter().copied())
        .map(win_percent_from_cp)
        .collect();

    let n = all_wp.len(); // cps.len() + 1
    if n < 2 {
        return None;
    }

    // Window size for std-dev weighting: (n_moves / 10) clamped to [2, 8].
    let window_size = (cps.len() / 10).clamp(2, 8);

    // Build the weight windows: (window_size.min(n) − 2) copies of the first
    // window, then all sliding windows of size window_size.min(n).
    // This produces exactly (n − 1) windows — one per move.
    let ws = window_size.min(n);
    let prefix_count = ws.saturating_sub(2);
    let first_win = &all_wp[..ws];

    let windows: Vec<&[f64]> = (0..prefix_count)
        .map(|_| first_win)
        .chain(all_wp.windows(ws))
        .collect();

    // Weight of each move = std-dev of its window, clamped to [0.5, 12].
    let weights: Vec<f64> = windows
        .iter()
        .map(|xs| std_dev(xs).unwrap_or(0.0).clamp(0.5, 12.0))
        .collect();

    // Compute per-color accuracy from consecutive win-percent pairs.
    //
    // For the i-th half-move (0-indexed):
    //   White: accuracy = fromWinPercents(prev, next)   — white wants wp to rise
    //   Black: accuracy = fromWinPercents(next, prev)   — equivalent to
    //          fromWinPercents(100−prev, 100−next) because the formula only
    //          depends on the magnitude of the win-percent change.
    let mut white_wt: Vec<(f64, f64)> = Vec::new();
    let mut black_wt: Vec<(f64, f64)> = Vec::new();
    let mut white_acc: Vec<f64> = Vec::new();
    let mut black_acc: Vec<f64> = Vec::new();

    for (i, (pair, &w)) in all_wp.windows(2).zip(weights.iter()).enumerate() {
        let (prev, next) = (pair[0], pair[1]);
        let is_white = (i % 2 == 0) == start_color_is_white;

        let acc = if is_white {
            accuracy_from_win_percents(prev, next)
        } else {
            accuracy_from_win_percents(next, prev)
        };

        if is_white {
            white_wt.push((acc, w));
            white_acc.push(acc);
        } else {
            black_wt.push((acc, w));
            black_acc.push(acc);
        }
    }

    // Final accuracy = average of weighted mean and harmonic mean.
    let wa = weighted_mean(&white_wt)
        .zip(harmonic_mean(&white_acc))
        .map(|(wm, hm)| (wm + hm) / 2.0)?;
    let ba = weighted_mean(&black_wt)
        .zip(harmonic_mean(&black_acc))
        .map(|(wm, hm)| (wm + hm) / 2.0)?;

    Some((wa, ba))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-9;

    // ── Helpers (private, but visible inside the module) ──────────────────────

    #[test]
    fn std_dev_empty() {
        assert!(std_dev(&[]).is_none());
    }

    #[test]
    fn std_dev_single() {
        // Single element — variance is 0.
        assert_eq!(std_dev(&[42.0]).unwrap(), 0.0);
    }

    #[test]
    fn std_dev_known() {
        // [2, 4, 4, 4, 5, 5, 7, 9]: population std-dev = 2.0 exactly.
        let xs = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let sd = std_dev(&xs).unwrap();
        assert!((sd - 2.0).abs() < 1e-9, "sd = {sd}");
    }

    #[test]
    fn weighted_mean_equal_weights() {
        // Equal weights → ordinary mean.
        let pairs = [(1.0f64, 1.0), (3.0, 1.0), (5.0, 1.0)];
        let wm = weighted_mean(&pairs).unwrap();
        assert!((wm - 3.0).abs() < EPS, "wm = {wm}");
    }

    #[test]
    fn weighted_mean_unequal() {
        // (10, weight=1) and (0, weight=9) → mean = 1.
        let pairs = [(10.0f64, 1.0), (0.0, 9.0)];
        let wm = weighted_mean(&pairs).unwrap();
        assert!((wm - 1.0).abs() < EPS, "wm = {wm}");
    }

    #[test]
    fn weighted_mean_empty() {
        assert!(weighted_mean(&[]).is_none());
    }

    #[test]
    fn harmonic_mean_known() {
        // HM(1, 2, 4) = 3 / (1 + 0.5 + 0.25) = 3 / 1.75 ≈ 1.7143
        let hm = harmonic_mean(&[1.0, 2.0, 4.0]).unwrap();
        let expected = 3.0 / (1.0 + 0.5 + 0.25);
        assert!((hm - expected).abs() < EPS, "hm = {hm}");
    }

    #[test]
    fn harmonic_mean_equal() {
        // HM of equal values = the value itself.
        let hm = harmonic_mean(&[7.0, 7.0, 7.0]).unwrap();
        assert!((hm - 7.0).abs() < EPS, "hm = {hm}");
    }

    #[test]
    fn harmonic_mean_empty() {
        assert!(harmonic_mean(&[]).is_none());
    }

    // ── win_percent_from_cp ───────────────────────────────────────────────────

    #[test]
    fn wp_equal_position() {
        // cp = 0 → exactly 50 %.
        assert!((win_percent_from_cp(0) - 50.0).abs() < EPS);
    }

    #[test]
    fn wp_initial_position() {
        // Cp.initial = 15 → slightly above 50 %.
        let wp = win_percent_from_cp(INITIAL_CP);
        assert!(wp > 50.0 && wp < 52.0, "wp = {wp}");
    }

    #[test]
    fn wp_known_value_100cp() {
        // cp = 100: 50 + 50*(2/(1+exp(-0.368208))-1) ≈ 59.1 %
        let wp = win_percent_from_cp(100);
        assert!((wp - 59.1).abs() < 0.1, "wp = {wp}");
    }

    #[test]
    fn wp_symmetry() {
        // wp(x) + wp(-x) == 100 for any x.
        for cp in [0, 15, 100, 300, 500, 1000, 1500] {
            let hi = win_percent_from_cp(cp);
            let lo = win_percent_from_cp(-cp);
            assert!(
                (hi + lo - 100.0).abs() < EPS,
                "cp={cp}: hi={hi}, lo={lo}, sum={}", hi + lo
            );
        }
    }

    #[test]
    fn wp_monotone_increasing() {
        let cps = [-1000, -500, -100, 0, 15, 100, 500, 1000];
        let wps: Vec<f64> = cps.iter().map(|&c| win_percent_from_cp(c)).collect();
        for w in wps.windows(2) {
            assert!(w[0] < w[1], "not monotone: {} >= {}", w[0], w[1]);
        }
    }

    #[test]
    fn wp_ceiling_clamp() {
        // Values beyond ±1000 should be clamped, giving the same result as ±1000.
        assert_eq!(win_percent_from_cp(1001), win_percent_from_cp(1000));
        assert_eq!(win_percent_from_cp(9999), win_percent_from_cp(1000));
        assert_eq!(win_percent_from_cp(-1001), win_percent_from_cp(-1000));
    }

    #[test]
    fn wp_range() {
        // Always in [0, 100].
        for cp in [-9999, -1000, -1, 0, 1, 1000, 9999] {
            let wp = win_percent_from_cp(cp);
            assert!(wp >= 0.0 && wp <= 100.0, "cp={cp}: wp={wp}");
        }
    }

    // ── accuracy_from_win_percents ────────────────────────────────────────────

    #[test]
    fn accuracy_no_loss_is_100() {
        // after >= before → 100 % in all cases.
        assert_eq!(accuracy_from_win_percents(50.0, 50.0), 100.0);
        assert_eq!(accuracy_from_win_percents(50.0, 60.0), 100.0);
        assert_eq!(accuracy_from_win_percents(0.0,  50.0), 100.0);
        assert_eq!(accuracy_from_win_percents(99.9, 100.0), 100.0);
    }

    #[test]
    fn accuracy_small_loss_near_100() {
        // win_diff = 1 → raw ≈ 95.6, accuracy ≈ 96.6 %
        let acc = accuracy_from_win_percents(51.0, 50.0);
        assert!(acc > 90.0 && acc < 100.0, "acc = {acc}");
    }

    #[test]
    fn accuracy_known_value_25pt_loss() {
        // win_diff = 25 → accuracy ≈ 32.6 %
        let acc = accuracy_from_win_percents(75.0, 50.0);
        assert!((acc - 32.6).abs() < 1.0, "acc = {acc}");
    }

    #[test]
    fn accuracy_massive_blunder() {
        // win_diff = 80 → accuracy close to 0 but not negative.
        let acc = accuracy_from_win_percents(90.0, 10.0);
        assert!(acc >= 0.0 && acc < 2.0, "acc = {acc}");
    }

    #[test]
    fn accuracy_never_negative() {
        // Accuracy is always ≥ 0 regardless of input.
        for (b, a) in [(100.0, 0.0), (99.0, 0.0), (80.0, 1.0), (60.0, 10.0)] {
            let acc = accuracy_from_win_percents(b, a);
            assert!(acc >= 0.0, "before={b}, after={a}: acc={acc}");
        }
    }

    #[test]
    fn accuracy_monotone_decreasing_with_loss() {
        // Larger win-percent loss → lower accuracy.
        let losses = [1.0, 5.0, 10.0, 20.0, 40.0, 60.0, 80.0];
        let accs: Vec<f64> = losses
            .iter()
            .map(|&d| accuracy_from_win_percents(90.0, 90.0 - d))
            .collect();
        for pair in accs.windows(2) {
            assert!(pair[0] > pair[1], "not monotone: {} <= {}", pair[0], pair[1]);
        }
    }

    // ── game_accuracy ─────────────────────────────────────────────────────────

    #[test]
    fn game_accuracy_empty_is_none() {
        assert!(game_accuracy(true, INITIAL_CP, &[]).is_none());
    }

    #[test]
    fn game_accuracy_one_move_is_none() {
        // Only one half-move means black never moved → no data for black → None.
        assert!(game_accuracy(true, INITIAL_CP, &[0]).is_none());
    }

    #[test]
    fn game_accuracy_constant_eval_is_perfect() {
        // All positions at Cp.initial: every consecutive pair is equal,
        // so every move scores 100 %.
        let cps = vec![INITIAL_CP; 40];
        let (wa, ba) = game_accuracy(true, INITIAL_CP, &cps).unwrap();
        assert!((wa - 100.0).abs() < EPS, "white = {wa}");
        assert!((ba - 100.0).abs() < EPS, "black = {ba}");
    }

    #[test]
    fn game_accuracy_result_in_range() {
        // Both outputs must be in [0, 100].
        let cps: Vec<i32> = (0..30).map(|i| (i * 17 % 200) - 100).collect();
        let (wa, ba) = game_accuracy(true, INITIAL_CP, &cps).unwrap();
        assert!(wa >= 0.0 && wa <= 100.0, "white = {wa}");
        assert!(ba >= 0.0 && ba <= 100.0, "black = {ba}");
    }

    #[test]
    fn game_accuracy_white_blunder_lowers_white_score() {
        // White blunders on move 1 (cp: 15 → -900).  Subsequent moves hold the
        // position at -900 so black keeps the advantage without blundering back.
        // White's accuracy should drop well below black's.
        let cps = vec![-900i32; 40];
        let (wa, ba) = game_accuracy(true, INITIAL_CP, &cps).unwrap();
        assert!(wa < ba, "expected white ({wa:.1}) < black ({ba:.1}) after white blunder");
        assert!(wa < 80.0, "white accuracy should drop noticeably: {wa}");
    }

    #[test]
    fn game_accuracy_black_blunder_lowers_black_score() {
        // Black blunders on move 1 (cp: -900 → +900 from white's view).
        // Subsequent moves hold the position so white keeps the advantage.
        let mut cps = vec![900i32; 40];
        cps[0] = -900; // After white's move 1 it's bad for white.
        // cps[1] = 900: black blunders, giving back all the advantage.
        let (wa, ba) = game_accuracy(true, INITIAL_CP, &cps).unwrap();
        assert!(ba < wa, "expected black ({ba:.1}) < white ({wa:.1}) after black blunder");
        assert!(ba < 80.0, "black accuracy should drop noticeably: {ba}");
    }

    #[test]
    fn game_accuracy_start_color_changes_result() {
        // Flipping start_color re-assigns which formula (white vs. black) is applied
        // to each move.  For a non-trivial game the outputs should differ.
        let cps: Vec<i32> = vec![-100, 100, -50, 150, 0, -200, 80, -80, 300, -300,
                                  -100, 100, -50, 150, 0, -200, 80, -80, 300, -300];
        let (wa_t, ba_t) = game_accuracy(true,  INITIAL_CP, &cps).unwrap();
        let (wa_f, ba_f) = game_accuracy(false, INITIAL_CP, &cps).unwrap();
        assert!(
            (wa_t - wa_f).abs() > 0.01 || (ba_t - ba_f).abs() > 0.01,
            "start_color had no effect: ({wa_t:.3},{ba_t:.3}) vs ({wa_f:.3},{ba_f:.3})"
        );
        // Both variants must still produce valid ranges.
        for v in [wa_t, ba_t, wa_f, ba_f] {
            assert!(v >= 0.0 && v <= 100.0, "out of range: {v}");
        }
    }
}
