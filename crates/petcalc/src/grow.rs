//! 檔次（成長檔）模型：由五個整數檔次 + 能力倍率推出各等級的 BP。

use crate::bp::{Bp, AXES};
use crate::rates::full_rate_or_zero;

/// 能力倍率預設值。絕大多數寵物是 0.2。
pub const DEFAULT_BPRATE: f64 = 0.2;

/// 隨機檔的總點數：五軸相加固定為 10。
pub const RANDOM_POOL: i32 = 10;

/// 單軸最多可能掉的檔數，決定 [`GrowRange::loop_range`] 的搜尋寬度。
///
/// 這是**預設值**不是硬上限 —— 原程式把它開在「檔次範圍」欄位裡讓使用者調，
/// 見 [`LossBounds`]。
pub const MAX_LOST_PER_AXIS: i32 = 4;

/// [`LossBounds`] 容許的掉檔上限。
///
/// ⚠️ **這是移植版自己加的護欄，原程式沒有。** 掉檔上限每加 1，檔次候選就
/// 多一個維度冪次（上限 `n` → `(n+1)^5` 組）：預設的 4 是 3125 組，
/// 放到 9 就是 100000 組。原程式是桌面程式，跑久了頂多卡住自己；
/// 網頁版跑在 Worker 裡，敞開讓人填 50 只會讓分頁像當掉一樣。
pub const MAX_LOSS_BOUND: i32 = 9;

/// 逐軸的掉檔範圍 `[最少, 最多]`，軸順序 `[HP, ATK, DEF, AGI, MP]`。
///
/// 對應原程式「檔次範圍」那一欄，預設 `4 4 4 4 4;0 0 0 0 0`
/// ＝ 每軸最多掉 [`MAX_LOST_PER_AXIS`] 檔、最少掉 0 檔。
/// [`Default`] 就是這組值，所以不去設定它的話搜尋範圍與先前完全一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LossBounds([[i32; 2]; AXES]);

impl Default for LossBounds {
    fn default() -> Self {
        Self([[0, MAX_LOST_PER_AXIS]; AXES])
    }
}

impl LossBounds {
    /// 逐軸建構，會夾進合法範圍：下界不小於 0、上界不超過 [`MAX_LOSS_BOUND`]，
    /// 且上下界寫反時自動對調（使用者手填的欄位，不該因為填反就無解）。
    pub fn new(lo: [i32; AXES], hi: [i32; AXES]) -> Self {
        Self(std::array::from_fn(|i| {
            let (a, b) = if lo[i] <= hi[i] {
                (lo[i], hi[i])
            } else {
                (hi[i], lo[i])
            };
            [a.clamp(0, MAX_LOSS_BOUND), b.clamp(0, MAX_LOSS_BOUND)]
        }))
    }

    /// 第 `axis` 軸的 `[最少, 最多]`。
    #[inline]
    pub const fn axis(&self, axis: usize) -> [i32; 2] {
        self.0[axis]
    }

    /// 拆成 `(下界陣列, 上界陣列)`。
    #[inline]
    pub fn split(&self) -> ([i32; AXES], [i32; AXES]) {
        (
            std::array::from_fn(|i| self.0[i][0]),
            std::array::from_fn(|i| self.0[i][1]),
        )
    }
}

/// 逐軸的隨機檔範圍 `[下界, 上界]`，軸順序 `[HP, ATK, DEF, AGI, MP]`。
///
/// 對應原程式「隨機檔範圍」那一欄，預設 `0 0 0 0 0;10 10 10 10 10`
/// ＝ 每軸 0..=[`RANDOM_POOL`]，也就是「只受總和為 10 這個條件限制」。
/// [`Default`] 就是這組值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RandomBounds([[i32; 2]; AXES]);

impl Default for RandomBounds {
    fn default() -> Self {
        Self([[0, RANDOM_POOL]; AXES])
    }
}

impl RandomBounds {
    /// 逐軸建構，夾進 `0..=RANDOM_POOL`，上下界寫反時自動對調。
    pub fn new(lo: [i32; AXES], hi: [i32; AXES]) -> Self {
        Self(std::array::from_fn(|i| {
            let (a, b) = if lo[i] <= hi[i] {
                (lo[i], hi[i])
            } else {
                (hi[i], lo[i])
            };
            [a.clamp(0, RANDOM_POOL), b.clamp(0, RANDOM_POOL)]
        }))
    }

    /// 第 `axis` 軸的 `[下界, 上界]`。
    #[inline]
    pub const fn axis(&self, axis: usize) -> [i32; 2] {
        self.0[axis]
    }

    /// 這組界限有沒有可能湊出總和 [`RANDOM_POOL`]？
    ///
    /// 下界總和 > 10 或上界總和 < 10 就一組都湊不出來。使用者填得太緊時
    /// 「無解」與「條件矛盾」看起來一模一樣，呼叫端要能分辨。
    pub fn is_satisfiable(&self) -> bool {
        let lo: i32 = self.0.iter().map(|b| b[0]).sum();
        let hi: i32 = self.0.iter().map(|b| b[1]).sum();
        lo <= RANDOM_POOL && RANDOM_POOL <= hi
    }

    /// 第 `axis` 軸能不能取值 `v`。
    #[inline]
    pub fn accepts(&self, axis: usize, v: i32) -> bool {
        self.0[axis][0] <= v && v <= self.0[axis][1]
    }
}

/// 一隻寵物的檔次組合（圖鑑值＝上限）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GrowRange {
    pub hp: i32,
    pub atk: i32,
    pub def: i32,
    pub agi: i32,
    pub mp: i32,
    pub bprate: f64,
}

/// [`GrowRange::calc_bp_at_level`] 的結果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BpAtLevel {
    /// 不含隨機檔與加點的 BP。
    pub base_bp: Bp,
    /// `sum(base_bp) + 10 * bprate` —— 含滿額隨機檔、不含加點。
    pub sum_base_bp: f64,
    /// `sum_base_bp + lvlpoint` —— 再加上等級點。
    pub sum_full_bp: f64,
}

impl GrowRange {
    pub const fn new(hp: i32, atk: i32, def: i32, agi: i32, mp: i32, bprate: f64) -> Self {
        Self {
            hp,
            atk,
            def,
            agi,
            mp,
            bprate,
        }
    }

    /// 軸順序 `[HP, ATK, DEF, AGI, MP]`。
    #[inline]
    pub const fn to_array(self) -> [i32; AXES] {
        [self.hp, self.atk, self.def, self.agi, self.mp]
    }

    #[inline]
    pub const fn from_array(a: [i32; AXES], bprate: f64) -> Self {
        Self::new(a[0], a[1], a[2], a[3], a[4], bprate)
    }

    #[inline]
    pub fn sum(self) -> i32 {
        self.hp + self.atk + self.def + self.agi + self.mp
    }

    /// 逐軸 `self <= other` —— 對應 cg-pet-calc 的 `GrowRange.contains`。
    ///
    /// 語意是「self 這組實際檔次，有可能是 other 這隻圖鑑寵掉檔後的結果」。
    /// 名字沿用上游，但方向容易看反：呼叫端是 `候選.contains(圖鑑)`。
    #[inline]
    pub fn contains(self, other: GrowRange) -> bool {
        self.hp <= other.hp
            && self.atk <= other.atk
            && self.def <= other.def
            && self.agi <= other.agi
            && self.mp <= other.mp
    }

    /// 某等級下的 BP。
    ///
    /// ```text
    /// base[i]     = tier[i] * bprate + fullRates[tier[i]] * (lvl - 1)
    /// sum_base_bp = Σ base + 10 * bprate          // 10 = 隨機檔總點數
    /// sum_full_bp = sum_base_bp + lvlpoint        // lvlpoint 預設 lvl - 1
    /// ```
    ///
    /// `lvlpoint` 傳 `None` 代表「等級點全部可用」，即 `lvl - 1`。
    pub fn calc_bp_at_level(self, lvl: i32, lvlpoint: Option<f64>) -> BpAtLevel {
        let lvldiff = f64::from(lvl - 1);
        let tiers = self.to_array();

        let mut bps = [0.0f64; AXES];
        for (i, &tier) in tiers.iter().enumerate() {
            bps[i] = f64::from(tier) * self.bprate + full_rate_or_zero(tier) * lvldiff;
        }

        let base_bp = Bp::from_array(bps);
        let sum_base_bp = base_bp.sum() + f64::from(RANDOM_POOL) * self.bprate;
        let lvlpoint = lvlpoint.unwrap_or(lvldiff);

        BpAtLevel {
            base_bp,
            sum_base_bp,
            sum_full_bp: sum_base_bp + lvlpoint,
        }
    }

    /// 列舉所有「掉檔後」的候選檔次組合。
    ///
    /// 圖鑑上的檔次是**上限**，實際個體只會等於或低於它，每軸最多掉
    /// [`MAX_LOST_PER_AXIS`] 檔，因此共 `5^5 = 3125` 組。
    /// 負數檔次會被跳過（`full_rate` 查不到）。
    pub fn loop_range(self, f: impl FnMut(GrowRange)) {
        self.loop_range_within(LossBounds::default(), f);
    }

    /// [`loop_range`](Self::loop_range) 的逐軸界限版 —— 對應原程式的「檔次範圍」。
    ///
    /// `bounds` 給的是**掉檔量**，所以該軸列舉的檔次是
    /// `[圖鑑值 - 最多掉, 圖鑑值 - 最少掉]`。傳 [`LossBounds::default`]
    /// 與 `loop_range` 完全等價。
    pub fn loop_range_within(self, bounds: LossBounds, mut f: impl FnMut(GrowRange)) {
        let tier = self.to_array();
        let (min_lost, max_lost) = bounds.split();
        // 掉得多 → 檔次低，所以下界配 max_lost、上界配 min_lost
        let lo: [i32; AXES] = std::array::from_fn(|i| tier[i] - max_lost[i]);
        let hi: [i32; AXES] = std::array::from_fn(|i| tier[i] - min_lost[i]);

        for a in lo[0]..=hi[0] {
            for b in lo[1]..=hi[1] {
                for c in lo[2]..=hi[2] {
                    for d in lo[3]..=hi[3] {
                        for e in lo[4]..=hi[4] {
                            if a < 0 || b < 0 || c < 0 || d < 0 || e < 0 {
                                continue;
                            }
                            f(GrowRange::new(a, b, c, d, e, self.bprate));
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 有一軸（mp=2）低於 MAX_LOST_PER_AXIS，用來涵蓋下界被裁的情況。
    fn sample() -> GrowRange {
        GrowRange::new(10, 8, 6, 4, 2, DEFAULT_BPRATE)
    }

    /// 所有軸都 >= MAX_LOST_PER_AXIS，不會有任何軸被裁。
    fn roomy() -> GrowRange {
        GrowRange::new(10, 9, 8, 7, 6, DEFAULT_BPRATE)
    }

    #[test]
    fn level_one_has_no_growth_contribution() {
        let g = sample();
        let r = g.calc_bp_at_level(1, None);
        // lvl 1 -> lvldiff 0 -> base 就只有 tier * bprate
        assert_eq!(r.base_bp.hp, 10.0 * 0.2);
        assert_eq!(r.base_bp.mp, 2.0 * 0.2);
        // 等級點為 0，所以 full == base
        assert_eq!(r.sum_full_bp, r.sum_base_bp);
    }

    #[test]
    fn growth_scales_with_level_minus_one() {
        let g = sample();
        let at1 = g.calc_bp_at_level(1, None);
        let at11 = g.calc_bp_at_level(11, None);
        // fullRates[10] = 0.415，10 級的成長 = 0.415 * 10
        let expected = at1.base_bp.hp + 0.415 * 10.0;
        assert!((at11.base_bp.hp - expected).abs() < 1e-12);
    }

    #[test]
    fn sum_base_includes_random_pool() {
        let g = sample();
        let r = g.calc_bp_at_level(1, None);
        let raw: f64 = r.base_bp.sum();
        assert!((r.sum_base_bp - (raw + 10.0 * 0.2)).abs() < 1e-12);
    }

    #[test]
    fn lvlpoint_defaults_to_level_minus_one() {
        let g = sample();
        let auto = g.calc_bp_at_level(50, None);
        let explicit = g.calc_bp_at_level(50, Some(49.0));
        assert_eq!(auto.sum_full_bp, explicit.sum_full_bp);

        let none_spent = g.calc_bp_at_level(50, Some(0.0));
        assert!((none_spent.sum_full_bp - auto.sum_full_bp + 49.0).abs() < 1e-12);
    }

    #[test]
    fn loop_range_yields_3125_candidates_when_no_axis_clamps() {
        let mut n = 0;
        roomy().loop_range(|_| n += 1);
        assert_eq!(n, 5 * 5 * 5 * 5 * 5);
    }

    /// 低檔次的軸會讓候選數變少（負檔次不合法）。
    #[test]
    fn loop_range_shrinks_when_an_axis_clamps() {
        let mut n = 0;
        sample().loop_range(|_| n += 1);
        // mp = 2 -> 只有 0,1,2 三個合法值
        assert_eq!(n, 5 * 5 * 5 * 5 * 3);
    }

    #[test]
    fn loop_range_skips_negative_tiers() {
        // mp = 2，所以 2-4 = -2，下界會被裁掉 -> 該軸只剩 0,1,2 三個合法值
        let mut mps = std::collections::BTreeSet::new();
        sample().loop_range(|g| {
            mps.insert(g.mp);
        });
        assert_eq!(mps.iter().copied().collect::<Vec<_>>(), vec![0, 1, 2]);
    }

    /// 預設界限必須跟改動前的 `loop_range` **逐項相同** ——
    /// 這條是「加了可調範圍但沒改行為」的守門員。
    #[test]
    fn default_loss_bounds_reproduce_the_old_hard_coded_sweep() {
        for g in [sample(), roomy()] {
            let mut old = Vec::new();
            g.loop_range(|c| old.push(c.to_array()));

            let mut new = Vec::new();
            g.loop_range_within(LossBounds::default(), |c| new.push(c.to_array()));

            assert_eq!(old, new, "預設界限改變了列舉結果");
        }
    }

    /// 收緊上限就少列舉：上限 n 的那一軸只剩 n+1 個值。
    #[test]
    fn a_tighter_max_loss_shrinks_the_sweep() {
        let g = roomy();
        for max in 0..=MAX_LOST_PER_AXIS {
            let mut n = 0;
            g.loop_range_within(LossBounds::new([0; AXES], [max; AXES]), |_| n += 1);
            assert_eq!(n, (max + 1).pow(5), "上限 {max} 的候選數不對");
        }
    }

    /// 下限也要生效：最少掉 2 檔就不該出現「一檔都沒掉」的候選。
    #[test]
    fn a_min_loss_excludes_the_undropped_candidate() {
        let g = roomy();
        let bounds = LossBounds::new([2; AXES], [4; AXES]);
        let mut saw_catalog_value = false;
        let mut n = 0;
        g.loop_range_within(bounds, |c| {
            n += 1;
            if c.to_array() == g.to_array() {
                saw_catalog_value = true;
            }
        });
        assert!(!saw_catalog_value, "下限 2 不該讓沒掉檔的組合出現");
        assert_eq!(n, 3i32.pow(5), "掉 2..=4 每軸應有 3 個值");
    }

    /// 使用者把上下界填反時對調，而不是回一組空的搜尋範圍。
    #[test]
    fn reversed_loss_bounds_get_swapped_not_emptied() {
        assert_eq!(
            LossBounds::new([4; AXES], [1; AXES]).axis(0),
            [1, 4],
            "填反了應該對調"
        );
    }

    /// 掉檔上限有護欄 —— 敞開讓人填會讓候選數爆炸（見 MAX_LOSS_BOUND）。
    #[test]
    fn loss_bounds_are_clamped_to_the_guard_rail() {
        let b = LossBounds::new([-5; AXES], [999; AXES]);
        assert_eq!(b.axis(0), [0, MAX_LOSS_BOUND]);
    }

    #[test]
    fn random_bounds_default_is_the_whole_pool() {
        assert_eq!(RandomBounds::default().axis(0), [0, RANDOM_POOL]);
        assert!(RandomBounds::default().is_satisfiable());
    }

    /// 下界總和 > 10 或上界總和 < 10 就湊不出總和為 10 的元組。
    #[test]
    fn contradictory_random_bounds_are_detected() {
        assert!(
            !RandomBounds::new([3; AXES], [10; AXES]).is_satisfiable(),
            "下界總和 15 > 10"
        );
        assert!(
            !RandomBounds::new([0; AXES], [1; AXES]).is_satisfiable(),
            "上界總和 5 < 10"
        );
        assert!(
            RandomBounds::new([2; AXES], [2; AXES]).is_satisfiable(),
            "剛好只有 2,2,2,2,2"
        );
    }

    #[test]
    fn loop_range_upper_bound_is_the_catalog_value() {
        let g = sample();
        g.loop_range(|c| {
            assert!(
                c.hp <= g.hp && c.atk <= g.atk && c.def <= g.def && c.agi <= g.agi && c.mp <= g.mp
            );
        });
    }
}
