//! 比對容差 —— 移植自噬生・魔物觀測者 v3.12。
//!
//! ```text
//! tol = clamp((當前等級 - 入手等級) / 200.0, 0.01, 0.1)
//! ```
//!
//! 兩個界限是原程式內嵌的 x87 80-bit `Extended` 浮點常數，解出來分別是
//! [`TOL_MIN`]（0.01）和 [`TOL_MAX`]（0.1）。
//!
//! # ⚠️ 這個容差不是「模糊比對」
//!
//! 移植初期把它理解成「比對能力值時整段放寬」，那是錯的（`CLAUDE.md` §3.4 有
//! 更正紀錄）。原程式比對能力值一律精確，容差只用在**檔次**上，而且只在
//! 該軸檔次落在 [`crate::rates::full_rate`] 五週期的不規則位置時才放寬：
//!
//! ```text
//! fullRates[n] = 0.04n + 0.01*floor((n-1)/5) + (n>0 && n%5==0 ? 0.005 : 0)
//!                                ^^^^^^^^^^^^   ^^^^^^^^^^^^^^^^^^^^^^^^^^
//!                                n%5==1 跳一階    n%5==0 多半階
//! ```
//!
//! 也就是說：**這是針對成長係數表 5 週期不規則處做的取整邊界修補**，
//! 不是給誤差開一道後門。規則見 [`tier_nudges`]。

/// 容差下限。
pub const TOL_MIN: f64 = 0.01;
/// 容差上限。
pub const TOL_MAX: f64 = 0.1;
/// 等級差的除數。
pub const TOL_DIVISOR: f64 = 200.0;

/// 每軸最多要試幾個偏移量（`檔次 % 5 == 0` 時是 `{0, -tol, +tol}`）。
pub const MAX_NUDGES: usize = 3;

/// `tol = clamp((hi - lo) / 200, 0.01, 0.1)`
///
/// `hi < lo` 時寬度為負，會被下限接住，等同於 [`TOL_MIN`]。
#[inline]
pub fn adaptive_tolerance(lo: f64, hi: f64) -> f64 {
    let raw = (hi - lo) / TOL_DIVISOR;
    if raw.is_nan() {
        return TOL_MIN;
    }
    raw.clamp(TOL_MIN, TOL_MAX)
}

/// 原程式實際餵給 [`adaptive_tolerance`] 的量：**當前等級減入手等級**。
///
/// 沒有入手等級時傳 `0`（原程式用 0 代表「不使用入手等級」）。
/// 等級跨越越多、成長誤差累積越大，容差就越寬 —— 但夾在 0.01 ~ 0.1 之間。
#[inline]
pub fn level_span_tolerance(lvl: i32, catch_lvl: i32) -> f64 {
    adaptive_tolerance(f64::from(catch_lvl), f64::from(lvl))
}

/// 某一軸的檔次落在 `fullRates` 五週期的哪個位置，決定取整前要試哪些偏移量。
///
/// 回傳 `(偏移量, 有效個數)`，第一個恆為 `0.0`（先試不偏移）：
///
/// | `檔次 % 5` | 係數表狀況                          | 偏移量            |
/// | ---------- | ----------------------------------- | ----------------- |
/// | 2, 3, 4    | 規律                                | `{0}`             |
/// | 1          | `floor((n-1)/5)` 剛跳一階，誤差單向 | `{0, -tol}`       |
/// | 0（n > 1） | 多了 `+0.005` 半階，值卡在整數邊界  | `{0, -tol, +tol}` |
///
/// `n = 0` 要走預設分支 —— 原程式的條件是 `n > 1 && n % 5 == 0`，
/// 跟 [`crate::rates::full_rate`] 閉合式的 `n = 0` 特判是同一件事。
#[inline]
pub fn tier_nudges(tier: i32, tol: f64) -> ([f64; MAX_NUDGES], usize) {
    match tier.rem_euclid(5) {
        0 if tier > 1 => ([0.0, -tol, tol], 3),
        1 => ([0.0, -tol, 0.0], 2),
        _ => ([0.0; MAX_NUDGES], 1),
    }
}

/// 帶容差的相等比較。
#[inline]
pub fn approx_eq(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

/// 帶容差的區間包含判斷（兩端都放寬）。
#[inline]
pub fn within(value: f64, lo: f64, hi: f64, tol: f64) -> bool {
    value >= lo - tol && value <= hi + tol
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_match_the_binary() {
        // 這兩個值由原程式的浮點常數解出，不是目測猜的 —— 早期曾誤判為 0.005 / 0.05。
        assert_eq!(TOL_MIN, 0.01);
        assert_eq!(TOL_MAX, 0.1);
    }

    #[test]
    fn narrow_range_hits_lower_bound() {
        // (1 - 0) / 200 = 0.005 < 0.01
        assert_eq!(adaptive_tolerance(0.0, 1.0), TOL_MIN);
        assert_eq!(adaptive_tolerance(0.0, 0.0), TOL_MIN);
    }

    #[test]
    fn wide_range_hits_upper_bound() {
        // (100 - 0) / 200 = 0.5 > 0.1
        assert_eq!(adaptive_tolerance(0.0, 100.0), TOL_MAX);
    }

    #[test]
    fn mid_range_scales_linearly() {
        // (8 - 0) / 200 = 0.04，落在 [0.01, 0.1] 之間
        assert!((adaptive_tolerance(0.0, 8.0) - 0.04).abs() < 1e-12);
        // 邊界：正好 0.01 與 0.1
        assert!((adaptive_tolerance(0.0, 2.0) - 0.01).abs() < 1e-12);
        assert!((adaptive_tolerance(0.0, 20.0) - 0.1).abs() < 1e-12);
    }

    #[test]
    fn inverted_range_falls_back_to_minimum() {
        assert_eq!(adaptive_tolerance(50.0, 0.0), TOL_MIN);
    }

    #[test]
    fn approx_eq_accepts_inside_rejects_outside() {
        assert!(approx_eq(1.0, 1.009, 0.01));
        assert!(approx_eq(1.0, 0.991, 0.01));
        assert!(!approx_eq(1.0, 1.02, 0.01));
        // 注意：剛好落在界線上時不保證成立 —— (1.0 - 1.01).abs()
        // 在 f64 下是 0.010000000000000009，比 0.01 大。
        // 所以呼叫端不該把 tol 當成精確界線用。
        assert!(!approx_eq(1.0, 1.01, 0.01));
    }

    #[test]
    fn within_widens_both_ends() {
        assert!(within(0.99, 1.0, 2.0, 0.01));
        assert!(within(2.01, 1.0, 2.0, 0.01));
        assert!(!within(0.98, 1.0, 2.0, 0.01));
    }

    #[test]
    fn level_span_is_what_feeds_the_formula() {
        // 沒填入手等級（0）→ 就是當前等級除以 200
        assert!((level_span_tolerance(8, 0) - 0.04).abs() < 1e-12);
        // 1 級新寵：1/200 = 0.005，被下限接住
        assert_eq!(level_span_tolerance(1, 0), TOL_MIN);
        // 59 級野寵：59/200 = 0.295，被上限接住 —— 跟實測殘差 0.07 的結論相容
        assert_eq!(level_span_tolerance(59, 0), TOL_MAX);
        // 40 級抓、練到 45：只跨 5 級 → 5/200 = 0.025，遠比同樣 45 級的家寵（0.1）緊
        assert!((level_span_tolerance(45, 40) - 0.025).abs() < 1e-12);
        assert_eq!(level_span_tolerance(45, 0), TOL_MAX);
        // 跨 2 級以下才會被下限接住
        assert_eq!(level_span_tolerance(42, 40), TOL_MIN);
    }

    /// 偏移量的分歧點必須**正好**落在 `fullRates` 閉合式不規則的兩個餘數上。
    ///
    /// 這條測試是把 `tier_nudges` 綁在 [`crate::rates::full_rate_formula`] 上：
    /// 只要哪天有人改了其中一邊而沒改另一邊，這裡就會紅。
    #[test]
    fn the_nudged_tiers_are_exactly_where_the_rate_table_is_irregular() {
        use crate::rates::full_rate_formula;

        for tier in 2..=crate::rates::MAX_TIER as i32 {
            let step = full_rate_formula(tier as u32) - full_rate_formula(tier as u32 - 1);
            let (_, count) = tier_nudges(tier, 0.1);
            // 規律處每階恰好 +0.04；不規律處是 +0.045（n%5==0）或 +0.05（n%5==1）
            let regular = (step - 0.04).abs() < 1e-9;
            assert_eq!(
                regular,
                count == 1,
                "檔次 {tier}：每階 {step} 與偏移量個數 {count} 對不起來"
            );
        }
    }

    #[test]
    fn nudge_sets_match_the_original_s_three_branches() {
        let t = 0.1;
        // n % 5 == 0 且 n > 1 → 三次：不偏移、-tol、+tol
        assert_eq!(tier_nudges(5, t), ([0.0, -t, t], 3));
        assert_eq!(tier_nudges(50, t), ([0.0, -t, t], 3));
        // n % 5 == 1 → 兩次：不偏移、-tol（原程式只有 fsubr，沒有 fadd）
        assert_eq!(tier_nudges(1, t).1, 2);
        assert_eq!(tier_nudges(51, t).1, 2);
        assert_eq!(tier_nudges(51, t).0[1], -t);
        // 其餘一律精確
        for tier in [2, 3, 4, 7, 48, 49] {
            assert_eq!(tier_nudges(tier, t).1, 1, "檔次 {tier} 不該放寬");
        }
        // n = 0 走預設分支 —— 原程式的條件帶著 `n > 1`
        assert_eq!(tier_nudges(0, t).1, 1);
    }

    /// 第一個偏移量恆為 0：先試「完全不偏移」，跟原程式的 `Trunc(v)` 打頭陣一致。
    #[test]
    fn the_first_nudge_is_always_zero() {
        for tier in 0..=crate::rates::MAX_TIER as i32 {
            assert_eq!(tier_nudges(tier, 0.1).0[0], 0.0);
        }
    }
}
