//! 「噬生・魔物觀測者」檔次計算核心。
//!
//! 移植自噬生・魔物觀測者 v3.12，以 `cg-pet-calc` 作為正向計算的對照基準。
//! 計算規格與資料來源見專案根目錄的 `CLAUDE.md`。
//!
//! ## 軸順序
//!
//! 五個 BP 軸一律是 `[HP, ATK, DEF, AGI, MP]`（體力／力量／強度／速度／魔法）。
//! 顯示用的「血魔攻防敏」是**另一個順序**，只在輸出層轉換。
//!
//! ## 與 cg-pet-calc 的差異（刻意的）
//!
//! * [`rates::FULL_RATES`] 是完整的 111 筆，cg-pet-calc 只有 53 筆且其中兩筆是錯的。
//! * [`tolerance`] 提供原程式的自適應容差比對，cg-pet-calc 沒有這個機制。
//! * [`stats`] 幫每組解算機率，cg-pet-calc 只做列舉、沒有量化。

pub mod bp;
pub mod grow;
pub mod guess;
pub mod matrix;
pub mod rates;
pub mod search;
pub mod stats;
pub mod tolerance;

pub use bp::{Bp, Stat, AXES};
pub use grow::{
    BpAtLevel, GrowRange, LossBounds, RandomBounds, DEFAULT_BPRATE, MAX_LOSS_BOUND,
    MAX_LOST_PER_AXIS, RANDOM_POOL,
};
pub use guess::{
    solve, Candidate, GuessOptions, GuessOutcome, ManualBounds, MatchMode, RandomSearch, Target,
};
pub use matrix::{lu_solve, stat_to_bp, M_BP, M_INV};
pub use rates::{full_rate, full_rate_or_zero, FULL_RATES, MAX_TIER};
pub use search::{points_needed, Query, Verdict, BEST_AXIS, BEST_GAIN, STATS};
pub use stats::{binomial, random_weight, Distribution, PERCENT_EPSILON};
pub use tolerance::{adaptive_tolerance, TOL_MAX, TOL_MIN};

#[cfg(test)]
mod integration_tests {
    use super::*;

    /// 端到端：一隻檔次全 0 的 1 級寵物 == 純基礎值。
    #[test]
    fn blank_pet_at_level_one() {
        let g = GrowRange::new(0, 0, 0, 0, 0, DEFAULT_BPRATE);
        let at = g.calc_bp_at_level(1, None);
        let stat = at.base_bp.calc_real_num();
        assert_eq!(stat.hp, 20);
        assert_eq!(stat.mp, 20);
        assert_eq!(stat.wis, 100);
    }

    /// 高檔次寵物必須用到 cg-pet-calc 缺的表格區段。
    #[test]
    fn high_tier_pet_uses_extended_rate_table() {
        // 檔次 60 在 cg-pet-calc 的表裡查不到（該表只到 52），會被當成 0 而算錯。
        let g = GrowRange::new(60, 60, 60, 60, 60, DEFAULT_BPRATE);
        let at = g.calc_bp_at_level(100, None);
        // fullRates[60] = 2.515，99 級成長 -> 60*0.2 + 2.515*99
        assert_eq!(FULL_RATES[60], 2.515);
        let expected = 60.0 * 0.2 + 2.515 * 99.0;
        assert!((at.base_bp.hp - expected).abs() < 1e-9);
        // 若誤用截斷表，成長項會是 0，血量會低非常多
        let stat = at.base_bp.calc_real_num();
        assert!(
            stat.hp > 2000,
            "hp={} 看起來像是用了截斷的 fullRates",
            stat.hp
        );
    }

    /// 等級越高能力越高（單調性），涵蓋整個檔次範圍。
    #[test]
    fn stats_increase_monotonically_with_level() {
        for tier in [0, 1, 25, 52, 53, 110] {
            let g = GrowRange::new(tier, tier, tier, tier, tier, DEFAULT_BPRATE);
            let mut prev = i64::MIN;
            for lvl in [1, 10, 50, 100, 255] {
                let s = g.calc_bp_at_level(lvl, None).base_bp.calc_real_num();
                assert!(s.hp >= prev, "tier={tier} lvl={lvl} hp 倒退了");
                prev = s.hp;
            }
        }
    }

    /// 容差在野寵情境下應該接得住整數列舉接不住的誤差。
    #[test]
    fn tolerance_accepts_small_float_drift() {
        let tol = adaptive_tolerance(0.0, 8.0); // 0.04
        assert!(tolerance::approx_eq(123.0, 123.03, tol));
        assert!(!tolerance::approx_eq(123.0, 123.5, tol));
    }
}
