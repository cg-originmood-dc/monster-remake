//! 候選解的機率統計 —— cg-pet-calc 完全沒有的一塊。
//!
//! [`crate::guess::solve`] 只會告訴你「有這 200 組解」。原程式更進一步：
//! 它幫每組解算了一個**權重**，正規化成百分比，於是使用者看到的是
//! 「這組檔次有 37.4% 的機率」而不是一長串等價的可能性。
//!
//! # 權重的來源
//!
//! 隨機檔是 5 個軸瓜分 10 點的結果。若這 10 點各自獨立、均勻地落進 5 個軸，
//! 得到某個特定 5 元組 `r` 的機率正比於**多項式係數**：
//!
//! ```text
//! w(r) = 10! / (r₀! r₁! r₂! r₃! r₄!)
//! ```
//!
//! 原程式的建表函式就是這樣算的 ——
//! 它把多項式係數拆成四個二項式連乘（`C(r4, r4) = 1`，所以第五項省略）：
//!
//! ```text
//! w = C(10, r0) × C(10-r0, r1) × C(10-r0-r1, r2) × C(10-r0-r1-r2, r3)
//! ```
//!
//! 接著除以總和再乘 100，所以存進候選表的就是**百分比**，不是原始係數。這裡照做。
//!
//! # 候選記錄的排版
//!
//! 原程式每筆候選記錄存了 13 個浮點數，包含：
//!
//! - 五個軸的**負掉檔量** `−lost[i]`（不是檔次本身）
//! - 五個軸的**隨機檔** `r[i]`（總和 10）
//! - 由上游候選列帶下來的兩個值
//! - 權重（正規化後為百分比）
//!
//! 原程式每列配置 `381` 個記錄槽（`0x17d`）。`381` 正好是
//! 「每軸掉 0..4 檔、總掉檔數為 10」的組合數 —— 見
//! [`tests::the_binary_s_381_slot_allocation_is_the_loss_tuple_count`]。
//! 這是掉檔上限確實是 4 的獨立佐證。
//!
//! # 顯示門檻
//!
//! 原程式用 `0.01`（百分點）判定「等於最大值」與「等於 100%」，
//! 實際解出來就是 [`PERCENT_EPSILON`]。

use crate::bp::AXES;
use crate::grow::{GrowRange, MAX_LOST_PER_AXIS, RANDOM_POOL};
use crate::guess::Candidate;

/// 百分比比較的門檻。原程式內嵌的浮點常數，值為 0.01 個百分點。
pub const PERCENT_EPSILON: f64 = 0.01;

/// 二項式係數 `C(n, k)`，`n < k` 或 `k < 0` 回傳 0。
///
/// 對應原程式的二項式係數函式。原版把 `k = 0..10` 展開成 switch，
/// 這裡用逐步約分的迴圈 —— 數值相同但不會中途溢位。
pub fn binomial(n: i32, k: i32) -> u64 {
    if k < 0 || n < k {
        return 0;
    }
    // C(n,k) == C(n,n-k)，取小的那邊迴圈短
    let k = k.min(n - k) as u64;
    let n = n as u64;
    let mut acc: u64 = 1;
    for i in 0..k {
        // 先乘後除必定整除：acc 此時是 C(n, i)
        acc = acc * (n - i) / (i + 1);
    }
    acc
}

/// 隨機檔 5 元組的權重 ＝ 多項式係數 `10! / ∏ r_i!`。
///
/// 總和不是 [`RANDOM_POOL`] 或含負數時回傳 0（該元組不可能發生）。
///
/// 最大值是 `w([2,2,2,2,2]) = 113400`，全部 1001 組加起來是 `5^10`。
pub fn random_weight(random: [i32; AXES]) -> u64 {
    if random.iter().any(|&r| r < 0) || random.iter().sum::<i32>() != RANDOM_POOL {
        return 0;
    }
    let mut left = RANDOM_POOL;
    let mut acc: u64 = 1;
    // 最後一軸的 C(r4, r4) = 1，跟原程式一樣省略
    for &r in &random[..AXES - 1] {
        acc *= binomial(left, r);
        left -= r;
    }
    acc
}

/// 一批候選解的機率分布。
///
/// 所有百分比欄位的單位都是「百分點」（0..=100），與原程式的顯示一致。
#[derive(Debug, Clone, PartialEq)]
pub struct Distribution {
    /// 與輸入候選**同序**的機率（百分比）。總和為 100。
    pub percent: Vec<f64>,
    /// 正規化前的權重總和。0 表示沒有任何合法候選。
    pub total_weight: u64,
    /// `percent` 的最大值。
    pub max_percent: f64,
    /// 每軸掉檔量 `0..=4` 的邊際機率（百分比）。
    ///
    /// 對應原程式 `showdet` 功能累加的那張 5×5 表：`acc[軸][掉檔量] += 權重`。
    pub lost_marginal: [[f64; MAX_LOST_PER_AXIS as usize + 1]; AXES],
    /// 每軸隨機檔 `0..=10` 的邊際機率（百分比）。
    pub random_marginal: [[f64; RANDOM_POOL as usize + 1]; AXES],
}

impl Distribution {
    /// 統計一批候選解。`catalog` 是圖鑑檔次，用來還原逐軸掉檔量。
    ///
    /// 候選解可以重複（同一組檔次配不同加點會各佔一筆）—— 原程式也是這樣，
    /// 每一筆都獨立計權重。
    pub fn summarize(catalog: GrowRange, candidates: &[Candidate]) -> Self {
        let weights: Vec<u64> = candidates.iter().map(|c| random_weight(c.random)).collect();
        let total: u64 = weights.iter().sum();

        let scale = if total == 0 {
            0.0
        } else {
            100.0 / total as f64
        };
        let percent: Vec<f64> = weights.iter().map(|&w| w as f64 * scale).collect();

        let mut lost_marginal = [[0.0f64; MAX_LOST_PER_AXIS as usize + 1]; AXES];
        let mut random_marginal = [[0.0f64; RANDOM_POOL as usize + 1]; AXES];
        let cat = catalog.to_array();

        for (c, &p) in candidates.iter().zip(&percent) {
            let grow = c.grow.to_array();
            for axis in 0..AXES {
                let lost = cat[axis] - grow[axis];
                // 掉超過 4 檔理論上不會發生，但推算允許指定 target_grow，
                // 越界時寧可漏統計也不要 panic。
                if (0..=MAX_LOST_PER_AXIS).contains(&lost) {
                    lost_marginal[axis][lost as usize] += p;
                }
                let r = c.random[axis];
                if (0..=RANDOM_POOL).contains(&r) {
                    random_marginal[axis][r as usize] += p;
                }
            }
        }

        let max_percent = percent.iter().copied().fold(0.0, f64::max);
        Self {
            percent,
            total_weight: total,
            max_percent,
            lost_marginal,
            random_marginal,
        }
    }

    /// 機率最高的那些候選的索引（可能並列）。
    ///
    /// 對應原程式挑出「要列在結果視窗裡的解」的條件：`ABS(w - max) < 0.01`。
    pub fn best(&self) -> Vec<usize> {
        if self.total_weight == 0 {
            return Vec::new();
        }
        self.percent
            .iter()
            .enumerate()
            .filter(|(_, &p)| (p - self.max_percent).abs() < PERCENT_EPSILON)
            .map(|(i, _)| i)
            .collect()
    }

    /// 各軸「掉檔量已完全確定」時的掉檔數，不確定則為 `None`。
    ///
    /// 對應原程式 `showdet` 的判斷：五軸全部確定就沒東西好顯示。
    pub fn determined_lost(&self) -> [Option<i32>; AXES] {
        std::array::from_fn(|axis| {
            self.lost_marginal[axis]
                .iter()
                .position(|&p| (100.0 - p) < PERCENT_EPSILON)
                .map(|i| i as i32)
        })
    }

    /// 五軸掉檔量是否全部確定 —— 為真時原程式不顯示機率明細。
    pub fn is_fully_determined(&self) -> bool {
        self.determined_lost().iter().all(Option::is_some)
    }

    /// 滿足 `pred` 的候選機率總和（百分比）。
    ///
    /// 對應原程式機率統計的語意：把使用者輸入的過濾條件套在候選表上，
    /// 把符合者的權重加起來當成「符合機率」。
    pub fn probability_where(
        &self,
        candidates: &[Candidate],
        mut pred: impl FnMut(&Candidate) -> bool,
    ) -> f64 {
        candidates
            .iter()
            .zip(&self.percent)
            .filter(|(c, _)| pred(c))
            .map(|(_, &p)| p)
            .sum()
    }

    /// 實際檔次逐軸落在 `[lo, hi]` 內的機率（百分比）。
    ///
    /// 對應原程式機率統計的主要輸出：使用者在側欄填五組上下限，這裡回報命中率。
    /// （原程式的記錄存的是掉檔量，圖鑑檔次固定時兩者等價。）
    pub fn probability_in_grow_range(
        &self,
        candidates: &[Candidate],
        lo: [i32; AXES],
        hi: [i32; AXES],
    ) -> f64 {
        self.probability_where(candidates, |c| {
            let g = c.grow.to_array();
            (0..AXES).all(|i| lo[i] <= g[i] && g[i] <= hi[i])
        })
    }

    /// 指定數軸的實際檔次合計 `>= threshold` 的機率（百分比）。
    ///
    /// 對應原程式機率統計的第二個輸出，三個可選軸 ＋ 一個門檻
    /// （見 `CLAUDE.md` §3.5）。這裡放寬成任意軸集合。
    pub fn probability_axis_sum_at_least(
        &self,
        candidates: &[Candidate],
        axes: &[usize],
        threshold: i32,
    ) -> f64 {
        self.probability_where(candidates, |c| {
            let g = c.grow.to_array();
            axes.iter().map(|&i| g[i]).sum::<i32>() >= threshold
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grow::DEFAULT_BPRATE;
    use crate::guess::for_each_random_tuple;
    use crate::{solve, GuessOptions, MatchMode, RandomSearch, Target};

    fn c(n: i32, k: i32) -> u64 {
        binomial(n, k)
    }

    #[test]
    fn binomial_matches_pascals_triangle() {
        assert_eq!(c(0, 0), 1);
        assert_eq!(c(10, 0), 1);
        assert_eq!(c(10, 10), 1);
        assert_eq!(c(10, 1), 10);
        assert_eq!(c(10, 5), 252);
        assert_eq!(c(14, 4), 1001); // 隨機檔的組合數
                                    // n < k 時回傳 0（與原程式行為一致）
        assert_eq!(c(3, 5), 0);
        assert_eq!(c(10, -1), 0);
        // 遞迴關係
        for n in 1..20 {
            for k in 1..n {
                assert_eq!(c(n, k), c(n - 1, k - 1) + c(n - 1, k), "C({n},{k})");
            }
        }
    }

    #[test]
    fn multinomial_weight_hits_the_known_extremes() {
        // 10!/(2!^5) = 3628800/32
        assert_eq!(random_weight([2, 2, 2, 2, 2]), 113400);
        // 全押一軸只有一種排法
        assert_eq!(random_weight([10, 0, 0, 0, 0]), 1);
        assert_eq!(random_weight([0, 0, 0, 0, 10]), 1);
        // 10!/(9!·1!) = 10
        assert_eq!(random_weight([9, 1, 0, 0, 0]), 10);
        // 不合法的元組
        assert_eq!(random_weight([1, 1, 1, 1, 1]), 0, "總和不是 10");
        assert_eq!(random_weight([11, 0, 0, 0, -1]), 0, "有負數");
    }

    /// 權重確實構成 5^10 的機率測度 —— 這是「10 點各自均勻丟進 5 軸」
    /// 這個模型的充要條件，也是整個機率統計的地基。
    #[test]
    fn weights_sum_to_five_to_the_tenth_over_all_tuples() {
        let mut total: u64 = 0;
        let mut count = 0;
        for_each_random_tuple(|r| {
            total += random_weight(r);
            count += 1;
        });
        assert_eq!(count, 1001, "隨機檔元組數應為 C(14,4)");
        assert_eq!(total, 5u64.pow(10), "多項式定理：Σ 10!/∏r! = 5^10");
    }

    /// 原程式每列配置 381 個記錄槽（`0x17d`）。381 = 每軸掉 0..4 檔、
    /// 總掉檔 10 的組合數 —— 這是「掉檔上限 4」的獨立佐證。
    #[test]
    fn the_binary_s_381_slot_allocation_is_the_loss_tuple_count() {
        let m = MAX_LOST_PER_AXIS;
        let mut n = 0;
        for a in 0..=m {
            for b in 0..=m {
                for d in 0..=m {
                    for e in 0..=m {
                        let f = RANDOM_POOL - a - b - d - e;
                        if (0..=m).contains(&f) {
                            n += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(n, 0x17d, "應為 381（原程式配置的每列槽數）");
    }

    /// 拿真的推算結果做端到端統計。
    fn sample() -> (GrowRange, Vec<Candidate>) {
        let catalog = GrowRange::new(30, 25, 18, 20, 22, DEFAULT_BPRATE);
        let bp = catalog.calc_bp_at_level(20, None).base_bp.to_array();
        let random = [3, 1, 2, 2, 2];
        let x: [f64; AXES] = std::array::from_fn(|i| bp[i] + DEFAULT_BPRATE * random[i] as f64);
        let s = crate::Bp::from_array(x).calc_real_num();
        let out = solve(
            catalog,
            Target::new(20, s.hp, s.mp, s.atk, s.def, s.agi),
            GuessOptions {
                not_order_point: 19,
                mode: MatchMode::Exact,
                random: RandomSearch::Exhaustive,
                ..Default::default()
            },
        );
        assert!(!out.candidates.is_empty());
        (catalog, out.candidates)
    }

    #[test]
    fn percentages_sum_to_one_hundred() {
        let (catalog, cands) = sample();
        let d = Distribution::summarize(catalog, &cands);
        assert_eq!(d.percent.len(), cands.len());
        let sum: f64 = d.percent.iter().sum();
        assert!((sum - 100.0).abs() < 1e-9, "總和 {sum} != 100");
        assert!(d.max_percent > 0.0);
        assert!(d.percent.iter().all(|&p| p > 0.0), "合法候選不該有 0 權重");
    }

    #[test]
    fn marginals_each_sum_to_one_hundred() {
        let (catalog, cands) = sample();
        let d = Distribution::summarize(catalog, &cands);
        for axis in 0..AXES {
            let lost: f64 = d.lost_marginal[axis].iter().sum();
            let rand: f64 = d.random_marginal[axis].iter().sum();
            assert!((lost - 100.0).abs() < 1e-9, "軸 {axis} 掉檔邊際 = {lost}");
            assert!((rand - 100.0).abs() < 1e-9, "軸 {axis} 隨機檔邊際 = {rand}");
        }
    }

    #[test]
    fn best_picks_the_heaviest_candidates() {
        let (catalog, cands) = sample();
        let d = Distribution::summarize(catalog, &cands);
        let best = d.best();
        assert!(!best.is_empty());
        for &i in &best {
            assert!((d.percent[i] - d.max_percent).abs() < PERCENT_EPSILON);
        }
        // 沒被選中的一定嚴格比較小
        for (i, &p) in d.percent.iter().enumerate() {
            if !best.contains(&i) {
                assert!(p < d.max_percent, "第 {i} 組 {p} 不比最大值小卻沒入選");
            }
        }
    }

    /// 只有唯一解時，五軸都應該是「100% 確定」。
    #[test]
    fn a_single_candidate_is_fully_determined() {
        let (catalog, cands) = sample();
        let one = &cands[..1];
        let d = Distribution::summarize(catalog, one);
        assert_eq!(d.percent, vec![100.0]);
        assert!(d.is_fully_determined());
        let lost = catalog.to_array()[0] - one[0].grow.to_array()[0];
        assert_eq!(d.determined_lost()[0], Some(lost));
    }

    #[test]
    fn range_query_is_a_partition() {
        let (catalog, cands) = sample();
        let d = Distribution::summarize(catalog, &cands);

        // 全開的範圍 = 100%
        let all = d.probability_in_grow_range(&cands, [0; AXES], [999; AXES]);
        assert!((all - 100.0).abs() < 1e-9, "全開範圍應為 100%，得到 {all}");

        // 不可能的範圍 = 0%
        let none = d.probability_in_grow_range(&cands, [999; AXES], [999; AXES]);
        assert_eq!(none, 0.0);

        // 「第 0 軸 <= k」與「第 0 軸 > k」互補
        let cat = catalog.to_array();
        let k = cat[0] - 2;
        let mut lo_hi = [999; AXES];
        lo_hi[0] = k;
        let left = d.probability_in_grow_range(&cands, [0; AXES], lo_hi);
        let mut hi_lo = [0; AXES];
        hi_lo[0] = k + 1;
        let right = d.probability_in_grow_range(&cands, hi_lo, [999; AXES]);
        assert!(
            (left + right - 100.0).abs() < 1e-9,
            "{left} + {right} != 100"
        );
    }

    #[test]
    fn axis_sum_query_is_monotone_in_the_threshold() {
        let (catalog, cands) = sample();
        let d = Distribution::summarize(catalog, &cands);
        let axes = [0, 1, 4];
        let mut prev = 100.0;
        for t in 0..=catalog.sum() {
            let p = d.probability_axis_sum_at_least(&cands, &axes, t);
            assert!(
                p <= prev + 1e-9,
                "門檻 {t} 的機率 {p} 竟然比前一個 {prev} 高"
            );
            prev = p;
        }
        assert_eq!(
            d.probability_axis_sum_at_least(&cands, &axes, i32::MIN),
            100.0
        );
    }

    #[test]
    fn empty_input_does_not_divide_by_zero() {
        let d = Distribution::summarize(GrowRange::new(1, 1, 1, 1, 1, DEFAULT_BPRATE), &[]);
        assert_eq!(d.total_weight, 0);
        assert_eq!(d.max_percent, 0.0);
        assert!(d.best().is_empty());
        assert!(d.percent.is_empty());
        assert!(!d.is_fully_determined());
    }
}
