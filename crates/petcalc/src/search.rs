//! 寵物搜尋 —— 原程式「搜索不低於輸入能力的寵物」功能。
//!
//! 問題長這樣：給定**等級**與**七項能力下限**，哪些寵物有辦法達到？
//! 直覺上像是對圖鑑做 filter，但欄位裡有「等級」，就不可能只比檔次 ——
//! 每隻寵都得先正算一次，再看差多少、補得補不完。
//!
//! ## 原程式的算法
//!
//! 原程式驗證「等級 > 1」且「七項相加 > 0」後，對每隻寵累加**補到下限所需的 BP**。
//! 每項能力的缺口除以該項最有效率軸的係數（`BEST_GAIN`），加總後跟可用點數比較。
//! 例如：缺口 `g` 點回復 ÷ `0.8`（每點體力給 0.8 回復），缺口 `g` 點魔力 ÷ `10`
//! （每點魔法給 10 魔力）。可用點數 ＝ 等級 − 1，補得完就算命中。
//!
//! ## 移植版取捨（明講，不假裝是複刻）
//!
//! 原程式對某些能力有第二條分支，走**混加**而不是單軸。回復那條是
//! `缺口 / 0.6875`，同時把 `× 3/8` 記到魔法軸上 —— 因為
//! `0.8 − 0.3 × 3/8 = 0.6875` 而 `−0.3 + 0.8 × 3/8 = 0`，
//! 也就是「補回復又不讓精神掉下去」的 8:3 配比。挑哪一條的判斷條件的 UI 來源沒查出來。
//!
//! 這裡只實作**單軸最快**那條（[`BEST_AXIS`]）。理由：它是原程式的主路徑，
//! 而且是「最少要幾點」的下界 —— 混加只會更貴，所以單軸不會漏掉本來搜得到的寵物。
//! 混加分支若日後查清楚，是加一個模式參數，不會推翻現有結果。
//!
//! 另外，原程式比對時用的隨機檔來源沒查出來；
//! 這裡把它開成參數（[`Query::random`]），預設跟介面其他地方一樣是 `[2,2,2,2,2]`。

use crate::bp::{Bp, Stat, AXES};
use crate::grow::GrowRange;

/// 能力項數 —— 血 魔 攻 防 敏 精神 回復，順序同 [`Stat::to_array`]。
pub const STATS: usize = 7;

/// 每項能力**最有效率**的加點軸（索引為 `[HP, ATK, DEF, AGI, MP]`）。
///
/// 就是 §3.1 那張矩陣每列的 argmax。用 [`best_axis_is_the_argmax_of_the_matrix`]
/// 釘住，改矩陣而忘了改這裡會被抓到。
pub const BEST_AXIS: [usize; STATS] = [
    0, // 生命 <- 體力 8
    4, // 魔力 <- 魔法 10
    1, // 攻擊 <- 力量 2.7
    2, // 防禦 <- 強度 3
    3, // 敏捷 <- 速度 2
    4, // 精神 <- 魔法 0.8
    0, // 回復 <- 體力 0.8
];

/// [`BEST_AXIS`] 那一軸每點帶來的增益 —— 也就是原程式算缺口時的那幾個除數。
///
/// 其中 `10`（魔力）與 `0.8`（回復）在原程式是字面常數，可直接對照驗證。
pub const BEST_GAIN: [f64; STATS] = [8.0, 10.0, 2.7, 3.0, 2.0, 0.8, 0.8];

/// 一次搜尋的條件。七項下限用 `0` 表示不限 —— 原程式就是拿空字串轉整數
/// 得到 `0`，再用「七項相加 > 0」擋掉全空的查詢。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Query {
    /// 目標等級。小於 1 會被夾到 1。
    pub lvl: i32,
    /// 血 魔 攻 防 敏 精神 回復 的下限。
    pub floor: [i64; STATS],
    /// 假設的隨機檔，總和應為 [`RANDOM_POOL`](crate::grow::RANDOM_POOL)。
    pub random: [i32; AXES],
}

/// 一隻寵物對某個 [`Query`] 的判定結果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Verdict {
    /// 該寵在這個等級、**完全不加點**時的能力。
    pub base: Stat,
    /// 把每項補到下限所需的加點數合計。已達標則為 `0.0`。
    pub needed: f64,
    /// 可用的加點數 ＝ `lvl - 1`。
    pub available: f64,
}

impl Verdict {
    /// 補得完就是命中。
    ///
    /// 用 `<=` 加一個很小的寬容量：缺口是整數、係數多半不是（`2.7`、`0.8`），
    /// 商很容易是 `19.000000000000004` 這種剛好超過的東西。
    /// 原程式在 x87 80-bit 下不會這麼容易踩到，f64 會。
    #[inline]
    pub fn hit(&self) -> bool {
        self.needed <= self.available + 1e-9
    }

    /// 還差幾點。命中時為 `0.0`。
    ///
    /// 走 [`hit`](Self::hit) 而不是直接相減 —— 不然「命中但差 1.8e-15 點」
    /// 這種自打嘴巴的組合會漏出去。
    #[inline]
    pub fn shortfall(&self) -> f64 {
        if self.hit() {
            0.0
        } else {
            self.needed - self.available
        }
    }
}

impl Query {
    /// 有沒有設下限。全 `0` 的查詢原程式會直接不做事。
    #[inline]
    pub fn has_floor(&self) -> bool {
        self.floor.iter().any(|&v| v > 0)
    }

    /// 可用加點數 ＝ `max(1, lvl) - 1`。
    #[inline]
    pub fn available_points(&self) -> f64 {
        f64::from(self.lvl.max(1) - 1)
    }

    /// 評估一隻寵物。
    pub fn evaluate(&self, grow: GrowRange) -> Verdict {
        let lvl = self.lvl.max(1);
        let at = grow.calc_bp_at_level(lvl, None);

        // 不加點的 BP ＝ 基礎成長 ＋ 隨機檔（隨機檔一樣要乘 bprate）
        let mut bps = at.base_bp.to_array();
        for (i, &r) in self.random.iter().enumerate() {
            bps[i] += f64::from(r) * grow.bprate;
        }
        let base = Bp::from_array(bps).calc_real_num();

        Verdict {
            base,
            needed: points_needed(base, self.floor),
            available: self.available_points(),
        }
    }
}

/// 把 `base` 每項補到 `floor` 所需的加點數合計。
///
/// 逐項獨立換算再相加 —— 跟原程式一樣**不管交互作用**。加體力補回復的同時
/// 生命也會漲，這裡不會把那份折抵掉，所以算出來偏保守（偏貴）。
pub fn points_needed(base: Stat, floor: [i64; STATS]) -> f64 {
    let have = base.to_array();
    let mut total = 0.0;
    for i in 0..STATS {
        let gap = floor[i] - have[i];
        if gap > 0 {
            total += gap as f64 / BEST_GAIN[i];
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bp::matrix;
    use crate::grow::DEFAULT_BPRATE;

    fn q(lvl: i32, floor: [i64; STATS]) -> Query {
        Query {
            lvl,
            floor,
            random: [2, 2, 2, 2, 2],
        }
    }

    /// [`BEST_AXIS`] / [`BEST_GAIN`] 必須真的是矩陣每列的最大值。
    ///
    /// 這兩張表是手寫的，寫錯不會爆炸，只會讓搜尋悄悄放行或漏掉寵物。
    #[test]
    fn best_axis_is_the_argmax_of_the_matrix() {
        let rows = [
            matrix::HP,
            matrix::MP,
            matrix::ATK,
            matrix::DEF,
            matrix::AGI,
            matrix::WIS,
            matrix::RES,
        ];
        for (i, row) in rows.iter().enumerate() {
            let (axis, gain) =
                row.iter()
                    .enumerate()
                    .fold((0usize, f64::NEG_INFINITY), |(bi, bv), (j, &v)| {
                        if v > bv {
                            (j, v)
                        } else {
                            (bi, bv)
                        }
                    });
            assert_eq!(BEST_AXIS[i], axis, "第 {i} 項的最佳軸對不上矩陣");
            assert_eq!(BEST_GAIN[i], gain, "第 {i} 項的增益對不上矩陣");
        }
    }

    /// 原程式可直接核對的兩個除數。
    ///
    /// 魔力係數 10 與回復係數 0.8 在原程式是字面常數，是整個模型唯一能直接
    /// 跟原程式對帳的地方，別讓它漂掉。
    #[test]
    fn the_two_divisors_visible_in_the_binary_match() {
        assert_eq!(BEST_GAIN[1], 10.0, "魔力的除數應為 10");
        assert_eq!(BEST_GAIN[6], 0.8, "回復的除數應為 0.8");
    }

    /// 1 級沒有加點可用 —— 只有本來就達標的才算命中。
    #[test]
    fn level_one_has_no_points_to_spend() {
        let query = q(1, [0; STATS]);
        assert_eq!(query.available_points(), 0.0);

        let grow = GrowRange::new(30, 30, 30, 30, 30, DEFAULT_BPRATE);
        let v = query.evaluate(grow);
        assert!(v.hit(), "沒設下限時 1 級也該命中");

        let mut floor = [0; STATS];
        floor[0] = v.base.hp + 1; // 生命只要多 1 點就補不起來
        assert!(!q(1, floor).evaluate(grow).hit());
    }

    /// 缺口換算成點數：生命差 8 就是 1 點體力。
    #[test]
    fn a_gap_converts_at_the_matrix_rate() {
        let base = Stat {
            hp: 100,
            mp: 100,
            atk: 20,
            def: 20,
            agi: 20,
            wis: 100,
            res: 100,
        };
        let mut floor = [0; STATS];
        floor[0] = 108;
        assert_eq!(points_needed(base, floor), 1.0);
        floor[0] = 116;
        assert_eq!(points_needed(base, floor), 2.0);
        // 已達標的項不計
        floor[0] = 50;
        assert_eq!(points_needed(base, floor), 0.0);
    }

    /// 多項缺口是相加，不是取最大 —— 這是原程式的累加語意。
    #[test]
    fn gaps_across_stats_add_up() {
        let base = Stat {
            hp: 100,
            mp: 100,
            atk: 20,
            def: 20,
            agi: 20,
            wis: 100,
            res: 100,
        };
        let mut floor = [0; STATS];
        floor[0] = 108; // +8 生命 -> 1 點
        floor[1] = 120; // +20 魔力 -> 2 點
        assert_eq!(points_needed(base, floor), 3.0);
    }

    /// 等級越高越容易命中 —— 能力本身在長，可用點數也在長。
    #[test]
    fn a_higher_level_never_loses_a_hit() {
        let grow = GrowRange::new(40, 30, 20, 25, 35, DEFAULT_BPRATE);
        let floor = [800, 600, 120, 110, 90, 0, 0];
        let mut seen_hit = false;
        for lvl in 1..=120 {
            let hit = q(lvl, floor).evaluate(grow).hit();
            if hit {
                seen_hit = true;
            }
            assert!(!seen_hit || hit, "等級 {lvl} 反而搜不到了");
        }
        assert!(seen_hit, "這組下限在 120 級前應該要搜得到");
    }

    /// 檔次高的寵物不會比檔次低的難命中。
    #[test]
    fn a_better_pet_is_never_harder_to_hit() {
        let floor = [500, 400, 90, 80, 70, 0, 0];
        let weak = q(50, floor).evaluate(GrowRange::new(20, 20, 20, 20, 20, DEFAULT_BPRATE));
        let strong = q(50, floor).evaluate(GrowRange::new(40, 40, 40, 40, 40, DEFAULT_BPRATE));
        assert!(strong.needed <= weak.needed);
    }

    /// 剛好整除的缺口不會被浮點誤差擋掉。
    ///
    /// 前半段是既有事實：現在這組係數（`8 / 10 / 2.7 / 3 / 2 / 0.8`）除下去
    /// 整除時就是整除，f64 不會溢出 —— 窮舉過 1..4000 的缺口，一次都沒有。
    /// 後半段測 [`Verdict::hit`] 的寬容量：那是給日後換係數用的保險
    /// （`1/3`、`0.7` 這種除數就會溢出），現在踩不到，但拆掉會變成定時炸彈。
    #[test]
    fn a_gap_that_divides_exactly_is_not_lost_to_float_error() {
        for (i, &gain) in BEST_GAIN.iter().enumerate() {
            for gap in 1..4000 {
                let q = f64::from(gap) / gain;
                assert!(
                    q <= q.round() || (q - q.round()).abs() > 1e-9,
                    "第 {i} 項：缺口 {gap} / {gain} 溢出成 {q}，hit() 的寬容量就變成必要的了"
                );
            }
        }

        let base = Stat::default();
        let v = Verdict {
            base,
            needed: 10.0 + f64::EPSILON * 8.0,
            available: 10.0,
        };
        assert!(v.hit(), "只超出一個浮點誤差不該算補不完");
        assert_eq!(v.shortfall(), 0.0);
        assert!(
            !Verdict {
                base,
                needed: 10.5,
                available: 10.0
            }
            .hit(),
            "真的差半點就是差"
        );
    }

    /// 全空的查詢要認得出來 —— 原程式在這種情況直接不做事。
    #[test]
    fn an_empty_query_is_recognisable() {
        assert!(!q(50, [0; STATS]).has_floor());
        assert!(q(50, [1, 0, 0, 0, 0, 0, 0]).has_floor());
    }
}
