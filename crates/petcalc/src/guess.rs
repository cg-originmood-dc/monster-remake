//! 推算引擎：由觀測到的能力值反推「實際檔次 ＋ 隨機檔 ＋ 加點分布」。
//!
//! 這是整個移植最有價值的一塊 —— 使用者的原話是「對於野寵跟一些推算，
//! 魔物觀測站做的還是比較好」。
//!
//! # 解的結構
//!
//! 一隻寵物在某等級的 BP 由三項相加而成：
//!
//! ```text
//! bp[i] = 檔次[i] * bprate + fullRates[檔次[i]] * (lvl - 1)   // 基礎成長
//!       + bprate * random[i]                                  // 隨機檔，Σrandom = 10
//!       + manual[i]                                           // 玩家加點，Σmanual = point
//! ```
//!
//! 觀測到的只有取整後的五項能力，所以要反推 `(檔次, random, manual)` 三組未知數。
//! 搜尋分三層：
//!
//! 1. **檔次** — 圖鑑值是上限，每軸最多掉 4 檔 → `5^5 = 3125` 組（[`GrowRange::loop_range`]）
//! 2. **加點** — `(a,b,c,d,e)` 總和 = `point`，各軸受 softLimit 限制
//! 3. **隨機檔** — 用反矩陣代數估計後在 ±1 鄰域列舉（[`RandomSearch::Ball`]），
//!    或直接窮舉全部 1001 組（[`RandomSearch::Exhaustive`]）
//!
//! # 三種比對模式
//!
//! | 模式 | 作法 | 來源 |
//! |---|---|---|
//! | [`MatchMode::Exact`] | 取整後整數相等 | cg-pet-calc |
//! | [`MatchMode::Observer`] | 逐軸依 `檔次 % 5` 決定要不要試 ±容差 | **原程式** |
//! | [`MatchMode::Tolerant`] | 全軸一律 ± 容差 | 移植版自己的延伸 |
//!
//! 精確比對在野寵高等案例常常直接「無解」：等級一高，成長係數累積出來的浮點
//! 尾巴會把值推過取整邊界。`Observer` 與 `Tolerant` 都是為了接住這件事，
//! 差別在放寬的範圍。
//!
//! # ⚠️ 這裡的列舉路徑就是原程式的作法
//!
//! 本檔早期的檔頭寫過「原程式把容差用在區間二分搜尋上，不是這裡的列舉路徑」
//! —— **那是錯的**，是看反編譯看岔了。原程式的建表函式是 13 層巢狀 `for`
//! （分組 5 + 4 + 4：五軸檔次，加點與隨機檔各 4 個自由度），跟這裡同一類作法。
//! 更正紀錄在 `CLAUDE.md` §3.4。
//!
//! 真正的差別在**容差擺哪裡**：原程式比對能力值一律精確，容差只用在檔次上，
//! 而且只在該軸檔次落在 `fullRates` 五週期不規則處（`% 5 ∈ {0, 1}`）時才放寬。
//! 這條規則由 [`crate::tolerance::tier_nudges`] 實作，[`MatchMode::Observer`] 用它。
//!
//! ### ⚠️ 偏移量加在 BP 上，這一步是推論不是實證
//!
//! 原程式那五個帶容差的比對是「候選記錄裡第 i 個浮點值 ± tol 之後 `Trunc`，
//! 跟第 i 個輸入框比」，而分類用的檔次也是第 i 軸 —— **逐軸一對一**。
//! 移植版沒有那組輸入框（那是原程式候選表的事後篩選面板），所以偏移量改加在
//! 列舉路徑上同樣逐軸一對一的量，也就是 `bp[i]`。
//!
//! 這樣接的理由是**量綱對得上**：要修的誤差來自 `fullRates[檔次[i]] * (lvl-1)`，
//! 那一項就住在 `bp[i]` 裡，`0.005` 的半階也是 BP 單位。容差公式
//! `(等級差)/200` 同樣是 BP 單位 —— 59 級累積下來是 `58 × 0.005 = 0.29`，
//! 正好是會被上界 0.1 夾住的量級。
//!
//! 副作用是偏移會被正算矩陣放大：`bp[MP]` 動 0.1，魔力那一項就動 1.0（係數 10）。
//! 那不是 bug，是誤差本來就會這樣傳遞 —— 但也意味著高係數軸放寬得比較多。
//! **沒有實證的是原程式那五個浮點值到底是不是 BP**（候選記錄的 doubles[8..12]，
//! 語意未解）。若日後查清楚不是，要改的就是這裡。
//!
//! [`MatchMode::Tolerant`] 則是移植版早期照著錯誤理解做出來的東西。它比
//! `Observer` 寬，**不是原程式的行為**，但拿來當「真的推不出來時最後試一次」
//! 還是有用，所以保留。預設容差取 [`crate::tolerance::TOL_MAX`]（0.1）：
//! 拿 cg-pet-calc 測試套件裡「純白液態史萊姆 59 級」窮舉所有整數組合後，
//! 最小可達的最大單項誤差是 **0.07**，0.1 剛好蓋得住又不超過原程式自己的上界。
//! 要自訂請用 [`GuessOptions::tolerance`]。

use crate::tolerance::{level_span_tolerance, tier_nudges, MAX_NUDGES, TOL_MAX};

use crate::bp::{fix_pos, matrix as fwd, Bp, Stat, AXES, PROP_BASE};
use crate::grow::{GrowRange, LossBounds, RandomBounds, RANDOM_POOL};
use crate::matrix::{apply_inv, stat_to_bp};

/// 觀測到的寵物：等級 ＋ 五項能力（顯示順序 血魔攻防敏）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub lvl: i32,
    pub hp: i64,
    pub mp: i64,
    pub atk: i64,
    pub def: i64,
    pub agi: i64,
}

impl Target {
    pub const fn new(lvl: i32, hp: i64, mp: i64, atk: i64, def: i64, agi: i64) -> Self {
        Self {
            lvl,
            hp,
            mp,
            atk,
            def,
            agi,
        }
    }

    /// 能力順序 `[生命, 魔力, 攻擊, 防禦, 敏捷]`。
    #[inline]
    pub const fn to_array(self) -> [i64; AXES] {
        [self.hp, self.mp, self.atk, self.def, self.agi]
    }

    /// 每項 +1 —— 用來算 BP 上界（觀測值是向下取整的，真值落在 `[n, n+1)`）。
    #[inline]
    pub const fn plus_one(self) -> Self {
        Self::new(
            self.lvl,
            self.hp + 1,
            self.mp + 1,
            self.atk + 1,
            self.def + 1,
            self.agi + 1,
        )
    }

    #[inline]
    fn to_bp(self) -> Bp {
        stat_to_bp(self.hp, self.mp, self.atk, self.def, self.agi)
    }
}

/// 比對模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MatchMode {
    /// 取整後整數精確相等 —— 與 cg-pet-calc 一致，可用於差分測試。
    Exact,
    /// **原程式的作法**：逐軸看檔次落在 `fullRates` 五週期的哪個位置，
    /// 決定取整前要不要試 ±容差（[`crate::tolerance::tier_nudges`]）。
    ///
    /// 容差本身是 `clamp((當前等級 − 入手等級)/200, 0.01, 0.1)`，
    /// 沒填入手等級時等級差就是當前等級。
    ///
    /// **這是預設值。** 這個 crate 存在的理由是重現原程式，所以沒指定模式時
    /// 就該走原程式那條；要 cg-pet-calc 的精確列舉（對拍時才需要）得明寫
    /// [`MatchMode::Exact`]。
    #[default]
    Observer,
    /// 全軸一律放寬 ±容差。**移植版自己的延伸，不是原程式的行為**，
    /// 見模組說明。比 [`MatchMode::Observer`] 寬。
    Tolerant,
    /// 由緊到鬆依序試 [`MatchMode::Exact`] → [`MatchMode::Observer`]
    /// → [`MatchMode::Tolerant`]，第一個有解的就回傳。
    Auto,
}

/// 隨機檔的搜尋策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RandomSearch {
    /// 用反矩陣代數估計後，在 ±`radius` 鄰域列舉（cg-pet-calc 的作法）。
    ///
    /// 快，但估計偏掉時會漏解 —— 而估計最容易偏的正是病態的野寵高等案例。
    Ball { radius: i32 },
    /// 窮舉所有總和為 10 的 5 元組（C(14,4) = 1001 組）。完備但慢。
    Exhaustive,
}

impl Default for RandomSearch {
    fn default() -> Self {
        Self::Ball { radius: 1 }
    }
}

/// 加點限制：逐軸的 `[下界, 上界]`，軸順序 `[HP, ATK, DEF, AGI, MP]`。
///
/// 這是原程式「運算模式」（智／野／無／體／力／防／敏／魔）與「加點方式」
/// （指定加點／混加）在搜尋層的共同表示 —— 那八種模式其實都只是不同的
/// 每軸上下界。收斂成一個表示之後，列舉迴圈可以直接拿它當邊界，
/// 順便把解空間砍小（見 `CLAUDE.md` §1.2）。
///
/// cg-pet-calc 沒有這個概念，只能無差別列舉所有分配。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManualBounds([[i32; 2]; AXES]);

impl Default for ManualBounds {
    fn default() -> Self {
        Self::free()
    }
}

impl ManualBounds {
    /// 逐軸 `[下界, 上界]`。
    pub const fn new(bounds: [[i32; 2]; AXES]) -> Self {
        Self(bounds)
    }

    /// 不限制 —— 運算模式「智」「野」。
    pub const fn free() -> Self {
        Self([[0, i32::MAX]; AXES])
    }

    /// 完全不加點 —— 運算模式「無」。
    ///
    /// 通常還要搭配 `not_order_point = lvl - 1`（點數是沒花掉，不是不存在）。
    pub const fn zero() -> Self {
        Self([[0, 0]; AXES])
    }

    /// 全部加在同一軸 —— 運算模式「體／力／防／敏／魔」。
    pub fn single(axis: usize) -> Self {
        let mut b = [[0, 0]; AXES];
        if axis < AXES {
            b[axis] = [0, i32::MAX];
        }
        Self(b)
    }

    /// 逐軸指定確切點數 —— 加點方式「指定加點」。
    pub fn fixed(points: [i32; AXES]) -> Self {
        Self(std::array::from_fn(|i| [points[i], points[i]]))
    }

    /// 只在選定的軸之間分配 —— 加點方式「混加」。
    pub fn mixed(axes: [bool; AXES]) -> Self {
        Self(std::array::from_fn(|i| {
            if axes[i] {
                [0, i32::MAX]
            } else {
                [0, 0]
            }
        }))
    }

    #[inline]
    pub fn lo(&self, axis: usize) -> i32 {
        self.0[axis][0]
    }

    #[inline]
    pub fn hi(&self, axis: usize) -> i32 {
        self.0[axis][1]
    }

    /// 這組加點是否合法。
    pub fn allows(&self, manual: [i32; AXES]) -> bool {
        (0..AXES).all(|i| manual[i] >= self.lo(i) && manual[i] <= self.hi(i))
    }

    /// 下界總和 —— 小於它的加點總數必定無解。
    pub fn min_total(&self) -> i32 {
        (0..AXES).map(|i| self.lo(i)).sum()
    }
}

/// 推算參數。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GuessOptions {
    /// 已知**未**分配的等級點數。野寵通常填 0（點是系統配的，但仍然配掉了）。
    pub not_order_point: i32,
    /// 指定實際檔次（跳過 3125 組列舉，只驗這一組）。
    pub target_grow: Option<GrowRange>,
    pub mode: MatchMode,
    pub random: RandomSearch,
    /// 加點的逐軸上下界。預設 [`ManualBounds::free`]（不限制）。
    pub manual: ManualBounds,
    /// 入手當下的觀測能力（野寵的「使用入手能力」）。`None` 代表不比對。
    ///
    /// 野寵在 N 級被抓到時，1→N 的等級點是系統配的、玩家一點也沒加
    /// （＝ `not_order_point = N - 1`），所以**入手當下的能力只由實際檔次與
    /// 隨機檔決定**，加點這一項完全不在裡面。
    ///
    /// 於是同一組 `(實際檔次, 隨機檔)` 必須同時吻合入手能力與當前能力 ——
    /// 這是一條很強的額外約束：當前能力那條式子有加點可以吸收誤差，入手那條
    /// 沒有，所以隨機檔常常被直接壓到唯一。這正是原程式「入手等級 ＋ 入手能力」
    /// 這組欄位存在的理由（`CLAUDE.md` §4.3）。
    pub catch: Option<Target>,
    /// 逐軸的掉檔範圍 —— 原程式的「檔次範圍」欄位。
    ///
    /// 預設 [`LossBounds::default`]（每軸 0..=4）＝ 先前寫死的行為。
    pub loss: LossBounds,
    /// 逐軸的隨機檔範圍 —— 原程式的「隨機檔範圍」欄位。
    ///
    /// 預設 [`RandomBounds::default`]（每軸 0..=10）＝ 只受總和為 10 限制。
    pub random_bounds: RandomBounds,
    /// 找到這麼多組解就停。`0` 代表不設限。
    pub limit: usize,
    /// 蓋掉自動算出來的比對容差。`None` 表示照模式自己的規則算。
    ///
    /// * [`MatchMode::Observer`] → `clamp((當前等級 − 入手等級)/200, 0.01, 0.1)`
    /// * [`MatchMode::Tolerant`] → [`TOL_MAX`]（0.1）
    /// * [`MatchMode::Exact`] 下這個欄位沒有作用。
    pub tolerance: Option<f64>,
}

impl GuessOptions {
    /// 該模式實際生效的容差。
    ///
    /// `Observer` 要看等級差，所以得知道當前等級 —— 入手等級從
    /// [`GuessOptions::catch`] 取，沒填就是 0（原程式用 0 代表不使用）。
    #[inline]
    fn tol(&self, mode: MatchMode, lvl: i32) -> f64 {
        match mode {
            MatchMode::Observer => self.tolerance.unwrap_or_else(|| {
                level_span_tolerance(lvl, self.catch.map_or(0, |c| c.lvl))
            }),
            MatchMode::Tolerant => self.tolerance.unwrap_or(TOL_MAX),
            // Auto 在 solve() 就已經具體化成其他三種之一
            MatchMode::Exact | MatchMode::Auto => 0.0,
        }
    }
}

/// 比對器：把「模式 ＋ 容差 ＋ 逐軸偏移集合」包成一個值。
///
/// 逐軸偏移只跟**該組候選檔次**有關，所以在 [`handle_grow_range`] 建一次就好，
/// 不用在加點／隨機檔的內圈重算。
#[derive(Debug, Clone, Copy)]
struct Matcher {
    mode: MatchMode,
    tol: f64,
    /// 逐軸允許的偏移量與其個數（[`MatchMode::Observer`] 專用）。
    nudges: [([f64; MAX_NUDGES], usize); AXES],
    /// 五軸是否全都只有一個偏移量（＝ 0）—— 那樣就跟 [`MatchMode::Exact`] 同路。
    all_exact: bool,
    /// 剪枝時每項能力要多留的寬放量。
    ///
    /// ⚠️ `Observer` 的偏移加在 **BP** 上，傳到能力值會被正算矩陣放大
    /// （生命／魔力那兩列的係數和是 17），拿 `tol` 當寬放量會把合法解剪掉。
    stat_slack: [f64; AXES],
    /// 剪枝時總 BP 要多留的寬放量 —— 每個可偏移的軸各 `tol`。
    sum_slack: f64,
}

impl Matcher {
    fn new(mode: MatchMode, tol: f64, grow: GrowRange) -> Self {
        let tiers = grow.to_array();
        let nudges: [([f64; MAX_NUDGES], usize); AXES] =
            std::array::from_fn(|i| tier_nudges(tiers[i], tol));
        let movable: [bool; AXES] = std::array::from_fn(|i| nudges[i].1 > 1);

        let (stat_slack, sum_slack) = match mode {
            // 容差直接定義在能力值上，寬放量就是它本身
            MatchMode::Tolerant => ([tol; AXES], tol),
            MatchMode::Observer => {
                let rows = [fwd::HP, fwd::MP, fwd::ATK, fwd::DEF, fwd::AGI];
                let slack = std::array::from_fn(|s| {
                    tol * (0..AXES)
                        .filter(|&i| movable[i])
                        .map(|i| rows[s][i].abs())
                        .sum::<f64>()
                });
                (slack, tol * movable.iter().filter(|m| **m).count() as f64)
            }
            MatchMode::Exact | MatchMode::Auto => ([0.0; AXES], 0.0),
        };

        Self {
            mode,
            tol,
            nudges,
            all_exact: nudges.iter().all(|(_, n)| *n == 1),
            stat_slack,
            sum_slack,
        }
    }

    /// 五項能力是否吻合。回傳 `Some(true)` 代表**靠偏移量才成立**。
    fn accepts(&self, bp: Bp, t: [i64; AXES]) -> Option<bool> {
        match self.mode {
            MatchMode::Tolerant => stats_within_band(bp, t, self.tol).then_some(true),
            MatchMode::Observer if !self.all_exact => self.accepts_nudged(bp, t),
            // Auto 不會走到這裡：solve() 已經把它具體化了
            _ => stats_equal(bp, t).then_some(false),
        }
    }

    /// 逐軸套偏移量後再比對，任何一組吻合就算數。
    ///
    /// 原程式對每個欄位獨立地試 `Trunc(v)` → `Trunc(v − tol)` → `Trunc(v + tol)`；
    /// 這裡的偏移加在 **BP** 上（每軸一個），所以要走過偏移量的笛卡兒積。
    /// 三軸以上同時落在不規則檔次才會超過 4 組，實務上多半只有 1～2 組。
    fn accepts_nudged(&self, bp: Bp, t: [i64; AXES]) -> Option<bool> {
        let base = bp.to_array();
        let counts: [usize; AXES] = std::array::from_fn(|i| self.nudges[i].1);
        let total: usize = counts.iter().product();
        for combo in 0..total {
            let mut rest = combo;
            let mut v = base;
            let mut nudged = false;
            for i in 0..AXES {
                let k = rest % counts[i];
                rest /= counts[i];
                if k != 0 {
                    v[i] += self.nudges[i].0[k];
                    nudged = true;
                }
            }
            if stats_equal(Bp::from_array(v), t) {
                return Some(nudged);
            }
        }
        None
    }
}

/// 一組推算結果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidate {
    /// 推出來的實際檔次。
    pub grow: GrowRange,
    /// 加點分布，軸順序 `[HP, ATK, DEF, AGI, MP]`。
    pub manual: [i32; AXES],
    /// 隨機檔分布，總和 10。
    pub random: [i32; AXES],
    /// 對應的 BP。
    pub bp: Bp,
    /// 由 `bp` 正算回去的能力值（含精神／回復 —— cg-pet-calc 會把這兩項丟掉）。
    pub stat: Stat,
    /// 總掉檔數 = 圖鑑總檔次 − 實際總檔次。
    pub lost_bp: i32,
    /// 逐軸的可能掉檔範圍 `[min, max]`。
    pub possible_lost: [[i32; 2]; AXES],
    /// 是否靠容差才成立（[`MatchMode::Exact`] 下恆為 `false`）。
    pub approximate: bool,
}

/// [`solve`] 的完整輸出。
#[derive(Debug, Clone, PartialEq)]
pub struct GuessOutcome {
    pub candidates: Vec<Candidate>,
    /// 實際生效的比對模式（`Auto` 會在這裡具體化成 `Exact` 或 `Tolerant`）。
    pub mode: MatchMode,
    /// 實際使用的加點總數。第一階段用 `lvl-1-not_order_point`，
    /// 無解時會退回 `0` 重試，這裡記錄最後成功的那個。
    pub point: i32,
}

/// 推算：給定圖鑑檔次與觀測能力，找出所有自洽的 `(檔次, 加點, 隨機檔)`。
///
/// 依 cg-pet-calc `guess()` 的三段策略，但第三段換成容差重試
/// （原版第三段是連續 LP 可行性回退 —— 那是因為它沒有容差模型才需要的補丁）。
pub fn solve(catalog: GrowRange, target: Target, opts: GuessOptions) -> GuessOutcome {
    let full_point = target.lvl - 1 - opts.not_order_point;

    // 依序嘗試 (加點總數, 比對模式)，第一個有解的就回傳
    let mut attempts: Vec<(i32, MatchMode)> = Vec::new();
    let ladder = |m: MatchMode, attempts: &mut Vec<(i32, MatchMode)>| {
        attempts.push((full_point, m));
        if target.lvl != 1 {
            attempts.push((0, m));
        }
    };
    match opts.mode {
        MatchMode::Exact | MatchMode::Observer | MatchMode::Tolerant => {
            ladder(opts.mode, &mut attempts);
        }
        // 由緊到鬆：精確 → 原程式的規則 → 全軸放寬
        MatchMode::Auto => {
            ladder(MatchMode::Exact, &mut attempts);
            ladder(MatchMode::Observer, &mut attempts);
            ladder(MatchMode::Tolerant, &mut attempts);
        }
    }

    for (point, mode) in attempts {
        if point < 0 {
            continue;
        }
        let candidates = search(catalog, target, point, mode, opts);
        if !candidates.is_empty() {
            return GuessOutcome {
                candidates,
                mode,
                point,
            };
        }
    }

    GuessOutcome {
        candidates: Vec::new(),
        mode: if opts.mode == MatchMode::Auto {
            MatchMode::Tolerant
        } else {
            opts.mode
        },
        point: full_point,
    }
}

fn search(
    catalog: GrowRange,
    target: Target,
    point: i32,
    mode: MatchMode,
    opts: GuessOptions,
) -> Vec<Candidate> {
    let mut out = Vec::new();

    // 觀測值取整前的 BP 區間：[toBP(stat), toBP(stat+1)]
    let o_sum = target.to_bp().sum();
    let up_bp = target.plus_one().to_bp();
    let o_up_sum = up_bp.sum();

    let mut visit = |grow: GrowRange| {
        if opts.limit > 0 && out.len() >= opts.limit {
            return;
        }
        if !grow.contains(catalog) {
            return;
        }
        handle_grow_range(
            grow, catalog, target, point, mode, opts, o_sum, o_up_sum, up_bp, &mut out,
        );
    };

    match opts.target_grow {
        Some(g) => visit(g),
        None => catalog.loop_range_within(opts.loss, visit),
    }

    out
}

#[allow(clippy::too_many_arguments)]
fn handle_grow_range(
    grow: GrowRange,
    catalog: GrowRange,
    target: Target,
    point: i32,
    mode: MatchMode,
    opts: GuessOptions,
    o_sum: f64,
    o_up_sum: f64,
    up_bp: Bp,
    out: &mut Vec<Candidate>,
) {
    let at = grow.calc_bp_at_level(target.lvl, Some(f64::from(point)));

    // 逐軸偏移只跟這組候選檔次有關，建一次給底下所有加點／隨機檔共用。
    let tol = opts.tol(mode, target.lvl);
    let matcher = Matcher::new(mode, tol, grow);

    // 總 BP 必須落在觀測區間內 —— 這一刀砍掉絕大多數候選
    if !(at.sum_full_bp >= o_sum - matcher.sum_slack
        && at.sum_full_bp <= o_up_sum + matcher.sum_slack)
    {
        return;
    }

    let base = at.base_bp.to_array();
    let up = up_bp.to_array();
    // softLimit：這一軸還能塞多少加點才不會超過觀測上界
    let soft: [f64; AXES] = std::array::from_fn(|i| up[i] - base[i] + 1.0);

    let bprate = grow.bprate;
    let t = target.to_array();
    let possible_lost = possible_lost_range(grow, catalog);
    let lost_bp = catalog.sum() - grow.sum();
    // 入手能力只跟 (檔次, 隨機檔) 有關，跟加點無關 —— 所以基礎 BP 在進加點迴圈前
    // 就算好，整個迴圈共用一份。
    let catch = opts.catch.map(|c| CatchCheck::new(grow, c));

    // 隨機檔對每項能力的貢獻界限：r 總和 10、各軸 0..10，
    // 所以 min(c·r) = min(c)*10、max(c·r) = max(c)*10。
    let rows = [fwd::HP, fwd::MP, fwd::ATK, fwd::DEF, fwd::AGI];
    let r_min: [f64; AXES] =
        std::array::from_fn(|s| rows[s].iter().cloned().fold(f64::MAX, f64::min) * 10.0 * bprate);
    let r_max: [f64; AXES] =
        std::array::from_fn(|s| rows[s].iter().cloned().fold(f64::MIN, f64::max) * 10.0 * bprate);

    // 加點的逐軸上下界（運算模式／加點方式）。
    let mb = opts.manual;
    if mb.min_total() > point {
        return; // 光下界就吃不完 —— 這組限制在這個等級不可能成立
    }
    // 上界＝softLimit、剩餘點數、模式上界，三者取小
    let cap = |i: usize| (soft[i].floor().min(f64::from(point)) as i32).min(mb.hi(i));
    // 決定第 i 軸時，後面的軸至少還要吃掉這麼多點
    let rest_lo = |i: usize| (i + 1..AXES).map(|k| mb.lo(k)).sum::<i32>();

    for a in mb.lo(0)..=cap(0).min(point - rest_lo(0)) {
        let rem_a = point - a;
        for b in mb.lo(1)..=cap(1).min(rem_a - rest_lo(1)) {
            let rem_b = rem_a - b;
            for c in mb.lo(2)..=cap(2).min(rem_b - rest_lo(2)) {
                let rem_c = rem_b - c;
                for d in mb.lo(3)..=cap(3).min(rem_c - rest_lo(3)) {
                    let e = rem_c - d;
                    if e < mb.lo(4) || e > mb.hi(4) || e < 0 || f64::from(e) > soft[4] {
                        continue;
                    }
                    let manual = [a, b, c, d, e];
                    let m: [f64; AXES] = std::array::from_fn(|i| base[i] + f64::from(manual[i]));

                    // 加點後的基礎能力（尚未加隨機檔）
                    let m_stat: [f64; AXES] = std::array::from_fn(|s| {
                        let row = &rows[s];
                        row[0] * m[0]
                            + row[1] * m[1]
                            + row[2] * m[2]
                            + row[3] * m[3]
                            + row[4] * m[4]
                    });

                    // 剪枝：隨機檔再怎麼配都補不到目標就跳過
                    let mut prunable = false;
                    for s in 0..AXES {
                        let slack = matcher.stat_slack[s];
                        let need_lo = (t[s] - PROP_BASE + 1) as f64 - m_stat[s] + slack;
                        let need_hi = (t[s] - PROP_BASE) as f64 - m_stat[s] - slack;
                        if need_lo <= r_min[s] || need_hi >= r_max[s] {
                            prunable = true;
                            break;
                        }
                    }
                    if prunable {
                        continue;
                    }

                    emit_random(
                        &m,
                        &m_stat,
                        bprate,
                        target,
                        matcher,
                        opts,
                        grow,
                        manual,
                        lost_bp,
                        possible_lost,
                        catch,
                        out,
                    );
                    if opts.limit > 0 && out.len() >= opts.limit {
                        return;
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_random(
    m: &[f64; AXES],
    m_stat: &[f64; AXES],
    bprate: f64,
    target: Target,
    matcher: Matcher,
    opts: GuessOptions,
    grow: GrowRange,
    manual: [i32; AXES],
    lost_bp: i32,
    possible_lost: [[i32; 2]; AXES],
    catch: Option<CatchCheck>,
    out: &mut Vec<Candidate>,
) {
    let t = target.to_array();
    let try_tuple = |r: [i32; AXES], out: &mut Vec<Candidate>| {
        // 上限要在**這裡**擋，不能只靠呼叫端。同一組（檔次, 加點）底下可以命中
        // 好幾組隨機檔，一次全推進去就會衝過頭 —— `limit: 5` 拿到 7 組就是這樣來的。
        // （`Observer` 比 `Exact` 容易多命中，所以是它先把這個洞踩出來的。）
        if opts.limit > 0 && out.len() >= opts.limit {
            return;
        }
        let x: [f64; AXES] = std::array::from_fn(|i| m[i] + bprate * f64::from(r[i]));
        let bp = Bp::from_array(x);
        let Some(mut nudged) = matcher.accepts(bp, t) else {
            return;
        };
        // 當前能力對得上還不夠：這組隨機檔在入手當下也得對得上。
        // 擺在後面是因為能走到這裡的候選已經很少了。
        if let Some(c) = catch {
            match c.accepts(r, bprate, &matcher) {
                Some(n) => nudged |= n,
                None => return,
            }
        }
        out.push(Candidate {
            grow,
            manual,
            random: r,
            bp,
            stat: bp.calc_real_num(),
            lost_bp,
            possible_lost,
            approximate: nudged,
        });
    };

    let rb = opts.random_bounds;
    match opts.random {
        RandomSearch::Exhaustive => {
            for_each_random_tuple_within(rb, |r| try_tuple(r, out));
        }
        RandomSearch::Ball { radius } => {
            // 代數初猜：r ≈ M_INV · (目標 - 19.5 - 加點後基礎) / bprate。
            // 用 19.5 而不是 20 是取整數格的中點，讓誤差對稱。
            let d: [f64; AXES] =
                std::array::from_fn(|s| ((t[s] as f64) - 19.5 - m_stat[s]) / bprate);
            let seed = apply_inv(d);
            let c: [i32; AXES] = std::array::from_fn(|i| crate::bp::js_round(seed[i]) as i32);

            for d0 in -radius..=radius {
                let r0 = c[0] + d0;
                if !rb.accepts(0, r0) {
                    continue;
                }
                for d1 in -radius..=radius {
                    let r1 = c[1] + d1;
                    if !rb.accepts(1, r1) {
                        continue;
                    }
                    for d2 in -radius..=radius {
                        let r2 = c[2] + d2;
                        if !rb.accepts(2, r2) {
                            continue;
                        }
                        for d3 in -radius..=radius {
                            let r3 = c[3] + d3;
                            if !rb.accepts(3, r3) {
                                continue;
                            }
                            let r4 = RANDOM_POOL - r0 - r1 - r2 - r3;
                            if !rb.accepts(4, r4) {
                                continue;
                            }
                            try_tuple([r0, r1, r2, r3, r4], out);
                        }
                    }
                }
            }
        }
    }
}

/// 入手能力的比對器（[`GuessOptions::catch`]）。
///
/// 入手等級的基礎 BP 只跟**實際檔次**有關，所以每組檔次候選算一次就好；
/// 之後每組隨機檔只剩一次 `base + bprate * r` 與一次比對。
#[derive(Debug, Clone, Copy)]
struct CatchCheck {
    /// 入手當下的觀測能力 `[生命, 魔力, 攻擊, 防禦, 敏捷]`。
    stats: [i64; AXES],
    /// 入手等級的基礎 BP。**不含加點** —— 那些點在入手當下還沒配。
    base: [f64; AXES],
}

impl CatchCheck {
    fn new(grow: GrowRange, catch: Target) -> Self {
        Self {
            stats: catch.to_array(),
            base: grow.calc_bp_at_level(catch.lvl, None).base_bp.to_array(),
        }
    }

    fn accepts(&self, random: [i32; AXES], bprate: f64, m: &Matcher) -> Option<bool> {
        let x: [f64; AXES] = std::array::from_fn(|i| self.base[i] + bprate * f64::from(random[i]));
        m.accepts(Bp::from_array(x), self.stats)
    }
}

/// 由 BP 正算出第 `s` 項能力的原始浮點值（取整前）。
#[inline]
fn raw_stat(v: &[f64; AXES], s: usize) -> f64 {
    let rows = [fwd::HP, fwd::MP, fwd::ATK, fwd::DEF, fwd::AGI];
    let row = &rows[s];
    row[0] * v[0] + row[1] * v[1] + row[2] * v[2] + row[3] * v[3] + row[4] * v[4]
}

/// 五項能力取整後是否與觀測值完全相等 —— 原程式比對能力值走的就是這條。
fn stats_equal(bp: Bp, t: [i64; AXES]) -> bool {
    let v = bp.to_array();
    (0..AXES).all(|s| fix_pos(raw_stat(&v, s)) + PROP_BASE == t[s])
}

/// 五項能力的原始值是否落在放寬後的整數格內（[`MatchMode::Tolerant`] 專用）。
///
/// 觀測值 n 代表真值落在 `[n-20, n-19)`，兩端各放寬 `tol`。
fn stats_within_band(bp: Bp, t: [i64; AXES], tol: f64) -> bool {
    let v = bp.to_array();
    (0..AXES).all(|s| {
        let raw = raw_stat(&v, s);
        let lo = (t[s] - PROP_BASE) as f64 - tol;
        let hi = (t[s] - PROP_BASE + 1) as f64 + tol;
        raw >= lo && raw < hi
    })
}

/// 走訪所有總和為 [`RANDOM_POOL`] 的 5 元組，共 C(14,4) = 1001 組。
pub fn for_each_random_tuple(f: impl FnMut([i32; AXES])) {
    for_each_random_tuple_within(RandomBounds::default(), f);
}

/// [`for_each_random_tuple`] 的逐軸界限版 —— 對應原程式的「隨機檔範圍」。
///
/// 總和恆為 [`RANDOM_POOL`] 這個條件不受界限影響，界限只是再把各軸夾窄。
/// 傳 [`RandomBounds::default`] 與 `for_each_random_tuple` 完全等價。
pub fn for_each_random_tuple_within(b: RandomBounds, mut f: impl FnMut([i32; AXES])) {
    // 每層的上界同時受「該軸自己的上界」與「剩餘點數」約束；下界則要扣掉
    // 後面各軸的下界總和，否則會走訪到湊不滿 10 的分支再白白丟掉。
    let need_after: [i32; AXES] =
        std::array::from_fn(|i| ((i + 1)..AXES).map(|j| b.axis(j)[0]).sum());

    let cap = |axis: usize, left: i32| b.axis(axis)[1].min(left - need_after[axis]);

    for r0 in b.axis(0)[0]..=cap(0, RANDOM_POOL) {
        let left1 = RANDOM_POOL - r0;
        for r1 in b.axis(1)[0]..=cap(1, left1) {
            let left2 = left1 - r1;
            for r2 in b.axis(2)[0]..=cap(2, left2) {
                let left3 = left2 - r2;
                for r3 in b.axis(3)[0]..=cap(3, left3) {
                    let r4 = left3 - r3;
                    if b.accepts(4, r4) {
                        f([r0, r1, r2, r3, r4]);
                    }
                }
            }
        }
    }
}

/// 逐軸的可能掉檔範圍 `[min, max]`。移植自 cg-pet-calc 的 `possibleLostRange`。
///
/// ⚠️ 上游有兩處可疑的地方，這裡**照原樣移植**以維持行為一致：
/// 1. `const min = maxBP[i] - 5` 算了但沒用到。
/// 2. `otherOverbase = sumSureBase - thisoverBase` 裡，`sumSureBase` 是夾過
///    下限 0 的總和，`thisoverBase` 卻是沒夾的原值；當該軸沒有超過圖鑑值時
///    （`thisoverBase < 0`）相減會把差值加回去，看起來不是本意。
pub fn possible_lost_range(grow: GrowRange, catalog: GrowRange) -> [[i32; 2]; AXES] {
    const MAX_BASE_POS: i32 = 10;

    let g = grow.to_array();
    let max_bp = catalog.to_array();

    let sum_sure_base: i32 = (0..AXES).map(|i| (g[i] - max_bp[i]).max(0)).sum();

    std::array::from_fn(|i| {
        let this_over_base = g[i] - max_bp[i];
        let other_over_base = sum_sure_base - this_over_base;
        let local_max = MAX_BASE_POS - other_over_base;
        [
            (max_bp[i] - g[i]).max(0),
            (max_bp[i] + local_max - g[i]).min(crate::grow::MAX_LOST_PER_AXIS),
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grow::DEFAULT_BPRATE;

    /// 由已知的 (檔次, 加點, 隨機檔) 正算出能力，再推回去，必須找得到原解。
    fn round_trip(
        grow: GrowRange,
        lvl: i32,
        manual: [i32; AXES],
        random: [i32; AXES],
    ) -> (Target, GuessOutcome) {
        let at = grow.calc_bp_at_level(lvl, None);
        let base = at.base_bp.to_array();
        let x: [f64; AXES] = std::array::from_fn(|i| {
            base[i] + grow.bprate * f64::from(random[i]) + f64::from(manual[i])
        });
        let s = Bp::from_array(x).calc_real_num();
        let target = Target::new(lvl, s.hp, s.mp, s.atk, s.def, s.agi);

        let point: i32 = manual.iter().sum();
        let opts = GuessOptions {
            not_order_point: lvl - 1 - point,
            ..Default::default()
        };
        (target, solve(grow, target, opts))
    }

    fn contains_solution(o: &GuessOutcome, manual: [i32; AXES], random: [i32; AXES]) -> bool {
        o.candidates
            .iter()
            .any(|c| c.manual == manual && c.random == random)
    }

    #[test]
    fn random_tuples_are_exactly_1001() {
        let mut n = 0;
        let mut all_sum_10 = true;
        for_each_random_tuple(|r| {
            n += 1;
            if r.iter().sum::<i32>() != RANDOM_POOL || r.iter().any(|&v| v < 0) {
                all_sum_10 = false;
            }
        });
        // C(10+4, 4) = 1001
        assert_eq!(n, 1001);
        assert!(all_sum_10);
    }

    #[test]
    fn recovers_a_level_one_pet() {
        let grow = GrowRange::new(20, 15, 12, 10, 18, DEFAULT_BPRATE);
        let random = [3, 2, 2, 2, 1];
        let (_, o) = round_trip(grow, 1, [0; AXES], random);
        assert!(!o.candidates.is_empty(), "1 級寵應該推得出來");
        assert!(contains_solution(&o, [0; AXES], random));
    }

    #[test]
    fn recovers_a_mid_level_pet_with_manual_points() {
        let grow = GrowRange::new(30, 25, 18, 20, 22, DEFAULT_BPRATE);
        let manual = [10, 5, 0, 3, 2];
        let random = [2, 2, 2, 2, 2];
        let (_, o) = round_trip(grow, 30, manual, random);
        assert!(!o.candidates.is_empty(), "無解");
        assert!(contains_solution(&o, manual, random), "找不到原本那組解");
    }

    /// 同一個觀測值，加上運算模式的限制之後，解必須是原本那批的**子集**
    /// —— 限制只能砍掉解，不能變出新的。
    #[test]
    fn manual_bounds_only_ever_shrink_the_solution_set() {
        let grow = GrowRange::new(30, 25, 18, 20, 22, DEFAULT_BPRATE);
        let manual = [12, 0, 0, 0, 0]; // 運算模式「體」：全加體力
        let random = [2, 2, 2, 2, 2];
        let (target, free) = round_trip(grow, 13, manual, random);
        assert!(
            contains_solution(&free, manual, random),
            "不限制時就該找得到"
        );

        let opts = GuessOptions {
            manual: ManualBounds::single(0),
            mode: free.mode,
            ..Default::default()
        };
        let bounded = solve(grow, target, opts);

        assert!(
            !bounded.candidates.is_empty(),
            "限制成「全加體力」之後反而無解"
        );
        assert!(
            contains_solution(&bounded, manual, random),
            "正解被限制擋掉了"
        );
        assert!(
            bounded.candidates.len() <= free.candidates.len(),
            "限制之後解變多了：{} > {}",
            bounded.candidates.len(),
            free.candidates.len()
        );
        for c in &bounded.candidates {
            assert_eq!(
                &c.manual[1..],
                &[0, 0, 0, 0],
                "非體力軸不該有加點：{:?}",
                c.manual
            );
        }
    }

    /// 「指定加點」把加點釘死成一組，解裡的加點就只能是那一組。
    #[test]
    fn fixed_manual_pins_every_axis() {
        let grow = GrowRange::new(30, 25, 18, 20, 22, DEFAULT_BPRATE);
        let manual = [4, 3, 2, 2, 1];
        let random = [2, 2, 2, 2, 2];
        let (target, _) = round_trip(grow, 13, manual, random);

        let o = solve(
            grow,
            target,
            GuessOptions {
                manual: ManualBounds::fixed(manual),
                ..Default::default()
            },
        );
        assert!(!o.candidates.is_empty(), "指定成正解本身卻無解");
        for c in &o.candidates {
            assert_eq!(c.manual, manual);
        }

        // 指定成別組就該一無所獲
        let wrong = solve(
            grow,
            target,
            GuessOptions {
                manual: ManualBounds::fixed([12, 0, 0, 0, 0]),
                ..Default::default()
            },
        );
        assert!(wrong.candidates.is_empty(), "指定了錯的加點卻還有解");
    }

    /// 「混加」只放行選定的軸。
    #[test]
    fn mixed_manual_restricts_to_the_selected_axes() {
        let grow = GrowRange::new(30, 25, 18, 20, 22, DEFAULT_BPRATE);
        let manual = [6, 6, 0, 0, 0];
        let random = [2, 2, 2, 2, 2];
        let (target, _) = round_trip(grow, 13, manual, random);

        let o = solve(
            grow,
            target,
            GuessOptions {
                manual: ManualBounds::mixed([true, true, false, false, false]),
                ..Default::default()
            },
        );
        assert!(
            contains_solution(&o, manual, random),
            "混加體＋力找不到正解"
        );
        for c in &o.candidates {
            assert_eq!(
                &c.manual[2..],
                &[0, 0, 0],
                "沒選的軸不該有加點：{:?}",
                c.manual
            );
        }
    }

    #[test]
    fn manual_bounds_constructors_agree_with_allows() {
        assert!(ManualBounds::free().allows([99, 0, 3, 0, 7]));
        assert!(ManualBounds::zero().allows([0; AXES]));
        assert!(!ManualBounds::zero().allows([1, 0, 0, 0, 0]));
        assert!(ManualBounds::single(2).allows([0, 0, 40, 0, 0]));
        assert!(!ManualBounds::single(2).allows([0, 1, 39, 0, 0]));
        assert!(ManualBounds::fixed([1, 2, 3, 4, 5]).allows([1, 2, 3, 4, 5]));
        assert!(!ManualBounds::fixed([1, 2, 3, 4, 5]).allows([1, 2, 3, 4, 6]));
        assert_eq!(ManualBounds::fixed([1, 2, 3, 4, 5]).min_total(), 15);
        assert_eq!(ManualBounds::free().min_total(), 0);

        let m = ManualBounds::mixed([false, true, false, true, false]);
        assert!(m.allows([0, 5, 0, 5, 0]));
        assert!(!m.allows([1, 5, 0, 4, 0]));
    }

    /// 下界總和超過可分配點數時要乾脆地回無解，而不是進迴圈亂算。
    #[test]
    fn impossible_bounds_return_nothing() {
        let grow = GrowRange::new(30, 25, 18, 20, 22, DEFAULT_BPRATE);
        let (target, _) = round_trip(grow, 5, [4, 0, 0, 0, 0], [2, 2, 2, 2, 2]);
        // 5 級只有 4 點，卻要求每軸至少 3 點
        let o = solve(
            grow,
            target,
            GuessOptions {
                manual: ManualBounds::fixed([3, 3, 3, 3, 3]),
                ..Default::default()
            },
        );
        assert!(o.candidates.is_empty());
    }

    /// 高檔次（cg-pet-calc 舊表查不到的區段）也要推得動。
    #[test]
    fn recovers_a_high_tier_pet() {
        let grow = GrowRange::new(60, 55, 70, 48, 52, DEFAULT_BPRATE);
        let random = [2, 2, 2, 2, 2];
        let (_, o) = round_trip(grow, 40, [0; AXES], random);
        assert!(!o.candidates.is_empty(), "高檔次寵推不出來");
        assert!(contains_solution(&o, [0; AXES], random));
    }

    /// 推出來的每一組解，正算回去都必須等於觀測值 —— 這是自洽性的底線。
    #[test]
    fn every_candidate_reproduces_the_observed_stats() {
        let grow = GrowRange::new(30, 25, 18, 20, 22, DEFAULT_BPRATE);
        let (target, o) = round_trip(grow, 30, [10, 5, 0, 3, 2], [2, 2, 2, 2, 2]);
        assert!(!o.candidates.is_empty());
        for c in &o.candidates {
            if c.approximate {
                continue; // 容差解允許差一格
            }
            assert_eq!(
                [c.stat.hp, c.stat.mp, c.stat.atk, c.stat.def, c.stat.agi],
                target.to_array(),
                "候選解 {:?} 正算回去對不上",
                c
            );
        }
    }

    /// 隨機檔恆為合法的 5 元組。
    #[test]
    fn every_candidate_has_a_legal_random_tuple() {
        let grow = GrowRange::new(30, 25, 18, 20, 22, DEFAULT_BPRATE);
        let (_, o) = round_trip(grow, 30, [10, 5, 0, 3, 2], [2, 2, 2, 2, 2]);
        for c in &o.candidates {
            assert_eq!(
                c.random.iter().sum::<i32>(),
                RANDOM_POOL,
                "隨機檔總和不是 10: {:?}",
                c.random
            );
            assert!(c.random.iter().all(|&r| (0..=RANDOM_POOL).contains(&r)));
            assert!(c.manual.iter().all(|&m| m >= 0));
        }
    }

    /// 窮舉法找到的解必須是 ±1 鄰域法的超集合。
    #[test]
    fn exhaustive_search_finds_at_least_as_much_as_the_ball() {
        let grow = GrowRange::new(30, 25, 18, 20, 22, DEFAULT_BPRATE);
        let at = grow.calc_bp_at_level(30, None);
        let base = at.base_bp.to_array();
        let random = [4, 1, 0, 3, 2];
        let x: [f64; AXES] = std::array::from_fn(|i| base[i] + grow.bprate * f64::from(random[i]));
        let s = Bp::from_array(x).calc_real_num();
        let target = Target::new(30, s.hp, s.mp, s.atk, s.def, s.agi);

        let base_opts = GuessOptions {
            not_order_point: 29,
            ..Default::default()
        };
        let ball = solve(grow, target, base_opts);
        let exhaustive = solve(
            grow,
            target,
            GuessOptions {
                random: RandomSearch::Exhaustive,
                ..base_opts
            },
        );

        assert!(!exhaustive.candidates.is_empty(), "窮舉都找不到就是有 bug");
        for c in &ball.candidates {
            assert!(
                exhaustive
                    .candidates
                    .iter()
                    .any(|e| e.grow == c.grow && e.manual == c.manual && e.random == c.random),
                "鄰域法找到但窮舉沒找到：{:?}",
                c
            );
        }
        assert!(exhaustive.candidates.len() >= ball.candidates.len());
    }

    /// 指定檔次時只驗那一組，不跑 3125 列舉。
    #[test]
    fn target_grow_restricts_the_search() {
        let grow = GrowRange::new(30, 25, 18, 20, 22, DEFAULT_BPRATE);
        let random = [2, 2, 2, 2, 2];
        let at = grow.calc_bp_at_level(20, None);
        let base = at.base_bp.to_array();
        let x: [f64; AXES] = std::array::from_fn(|i| base[i] + grow.bprate * f64::from(random[i]));
        let s = Bp::from_array(x).calc_real_num();
        let target = Target::new(20, s.hp, s.mp, s.atk, s.def, s.agi);

        let o = solve(
            grow,
            target,
            GuessOptions {
                not_order_point: 19,
                target_grow: Some(grow),
                ..Default::default()
            },
        );
        assert!(!o.candidates.is_empty());
        assert!(o.candidates.iter().all(|c| c.grow == grow));
    }

    /// 容差模式必須是精確模式的超集合。
    #[test]
    fn tolerant_mode_is_a_superset_of_exact() {
        let grow = GrowRange::new(30, 25, 18, 20, 22, DEFAULT_BPRATE);
        let at = grow.calc_bp_at_level(25, None);
        let base = at.base_bp.to_array();
        let random = [2, 2, 2, 2, 2];
        let x: [f64; AXES] = std::array::from_fn(|i| base[i] + grow.bprate * f64::from(random[i]));
        let s = Bp::from_array(x).calc_real_num();
        let target = Target::new(25, s.hp, s.mp, s.atk, s.def, s.agi);

        let opts = GuessOptions {
            not_order_point: 24,
            ..Default::default()
        };
        let exact = solve(
            grow,
            target,
            GuessOptions {
                mode: MatchMode::Exact,
                ..opts
            },
        );
        let tolerant = solve(
            grow,
            target,
            GuessOptions {
                mode: MatchMode::Tolerant,
                ..opts
            },
        );

        assert!(!exact.candidates.is_empty());
        for c in &exact.candidates {
            assert!(
                tolerant
                    .candidates
                    .iter()
                    .any(|x| x.grow == c.grow && x.manual == c.manual && x.random == c.random),
                "精確解 {:?} 在容差模式下反而不見了",
                c
            );
        }
    }

    /// `limit` 是硬上限，**每個模式都要守**。
    ///
    /// 這裡逐模式跑過一遍是有原因的：上限原本只在「換下一組（檔次, 加點）」時檢查，
    /// 而同一組底下可能命中好幾組隨機檔 —— 命中越多超得越兇。`Exact` 剛好不會超，
    /// 所以這個洞在預設值是 `Auto`（先走 `Exact`）的時候一直沒被看見。
    #[test]
    fn limit_caps_the_result_count() {
        let grow = GrowRange::new(44, 21, 14, 12, 29, DEFAULT_BPRATE);
        let target = Target::new(1, 122, 102, 36, 33, 28);
        for mode in [
            MatchMode::Exact,
            MatchMode::Observer,
            MatchMode::Tolerant,
            MatchMode::Auto,
        ] {
            for limit in [1, 3, 5] {
                let o = solve(
                    grow,
                    target,
                    GuessOptions {
                        limit,
                        mode,
                        ..Default::default()
                    },
                );
                assert!(
                    o.candidates.len() <= limit,
                    "{mode:?} limit={limit} 沒生效：{}",
                    o.candidates.len()
                );
            }
        }
    }

    /// 預設界限走訪出來的元組必須跟改動前的 1001 組**逐項相同**。
    #[test]
    fn default_random_bounds_reproduce_the_old_1001_tuples() {
        let mut old = Vec::new();
        for r0 in 0..=RANDOM_POOL {
            for r1 in 0..=(RANDOM_POOL - r0) {
                for r2 in 0..=(RANDOM_POOL - r0 - r1) {
                    for r3 in 0..=(RANDOM_POOL - r0 - r1 - r2) {
                        old.push([r0, r1, r2, r3, RANDOM_POOL - r0 - r1 - r2 - r3]);
                    }
                }
            }
        }

        let mut new = Vec::new();
        for_each_random_tuple_within(RandomBounds::default(), |r| new.push(r));

        assert_eq!(old.len(), 1001);
        assert_eq!(old, new, "預設界限改變了隨機檔的列舉結果");
    }

    /// 收窄界限只會從那 1001 組裡篩，不會冒出總和不是 10 的東西。
    #[test]
    fn narrowed_random_bounds_stay_a_subset_that_still_sums_to_ten() {
        let b = RandomBounds::new([1, 0, 0, 0, 1], [3, 4, 4, 4, 3]);

        let mut all = Vec::new();
        for_each_random_tuple(|r| all.push(r));
        let expected: Vec<_> = all
            .into_iter()
            .filter(|r| (0..AXES).all(|i| b.accepts(i, r[i])))
            .collect();

        let mut got = Vec::new();
        for_each_random_tuple_within(b, |r| got.push(r));

        assert_eq!(got, expected, "界限版應等於「全部列舉再過濾」");
        assert!(!got.is_empty());
        assert!(got.iter().all(|r| r.iter().sum::<i32>() == RANDOM_POOL));
    }

    /// 條件矛盾時一組都不該吐出來（而不是吐出違反界限的東西）。
    #[test]
    fn contradictory_random_bounds_yield_nothing() {
        let b = RandomBounds::new([3, 3, 3, 3, 3], [10, 10, 10, 10, 10]);
        assert!(!b.is_satisfiable());
        let mut n = 0;
        for_each_random_tuple_within(b, |_| n += 1);
        assert_eq!(n, 0, "下界總和 15 湊不出 10，不該有解");
    }

    /// 逐軸釘死成 2 2 2 2 2 就只剩那一組 —— 「隨機檔已知」的常見情況。
    #[test]
    fn pinning_every_axis_leaves_exactly_one_tuple() {
        let mut got = Vec::new();
        for_each_random_tuple_within(RandomBounds::new([2; AXES], [2; AXES]), |r| got.push(r));
        assert_eq!(got, vec![[2, 2, 2, 2, 2]]);
    }

    /// 掉檔範圍逐軸都要落在 `0..=MAX_LOST_PER_AXIS` 之內。
    #[test]
    fn possible_lost_range_is_within_bounds() {
        let catalog = GrowRange::new(30, 25, 18, 20, 22, DEFAULT_BPRATE);
        catalog.loop_range(|g| {
            let r = possible_lost_range(g, catalog);
            for (i, [lo, hi]) in r.iter().enumerate() {
                assert!(*lo >= 0, "軸 {i} 下界為負");
                assert!(*hi <= crate::grow::MAX_LOST_PER_AXIS, "軸 {i} 上界超過 4");
            }
        });
    }

    /// 沒掉檔時，逐軸的掉檔下界都是 0。
    #[test]
    fn possible_lost_range_is_zero_when_nothing_dropped() {
        let catalog = GrowRange::new(30, 25, 18, 20, 22, DEFAULT_BPRATE);
        let r = possible_lost_range(catalog, catalog);
        for [lo, _] in r {
            assert_eq!(lo, 0);
        }
    }

    // ── 入手能力 ────────────────────────────────────────────────────────────

    /// 某等級的能力值（正算）。
    fn stat_at(grow: GrowRange, lvl: i32, manual: [i32; AXES], random: [i32; AXES]) -> Stat {
        let base = grow.calc_bp_at_level(lvl, None).base_bp.to_array();
        let x: [f64; AXES] = std::array::from_fn(|i| {
            base[i] + grow.bprate * f64::from(random[i]) + f64::from(manual[i])
        });
        Bp::from_array(x).calc_real_num()
    }

    /// 一隻野寵：20 級被抓到，練到 40 級並把那 20 點配掉。
    ///
    /// 回傳 `(圖鑑檔次, 當前觀測, 入手觀測, 加點, 隨機檔, 不含入手能力的選項)`。
    fn wild_case() -> (
        GrowRange,
        Target,
        Target,
        [i32; AXES],
        [i32; AXES],
        GuessOptions,
    ) {
        let grow = GrowRange::new(30, 25, 18, 20, 22, DEFAULT_BPRATE);
        let random = [4, 1, 0, 3, 2];
        let manual = [8, 4, 3, 3, 2]; // 共 20 點 ＝ 40 - 1 - 19

        let now = stat_at(grow, 40, manual, random);
        let then = stat_at(grow, 20, [0; AXES], random); // 入手當下還沒加點

        let opts = GuessOptions {
            not_order_point: 19,
            random: RandomSearch::Exhaustive,
            ..Default::default()
        };
        (
            grow,
            Target::new(40, now.hp, now.mp, now.atk, now.def, now.agi),
            Target::new(20, then.hp, then.mp, then.atk, then.def, then.agi),
            manual,
            random,
            opts,
        )
    }

    /// 加上入手能力之後，正解還是要在解集合裡 —— 這條約束不能把真解濾掉。
    #[test]
    fn catch_stats_keep_the_true_solution() {
        let (grow, target, catch, manual, random, opts) = wild_case();
        let o = solve(
            grow,
            target,
            GuessOptions {
                catch: Some(catch),
                ..opts
            },
        );
        assert!(!o.candidates.is_empty(), "帶入手能力反而無解");
        assert!(
            contains_solution(&o, manual, random),
            "入手能力把正解濾掉了"
        );
    }

    /// 入手能力只能砍解，不能變出新解；而且要真的砍掉一些，否則這個欄位沒意義。
    #[test]
    fn catch_stats_only_ever_shrink_the_solution_set() {
        let (grow, target, catch, _, _, opts) = wild_case();
        let without = solve(grow, target, opts);
        let with = solve(
            grow,
            target,
            GuessOptions {
                catch: Some(catch),
                ..opts
            },
        );

        assert!(!without.candidates.is_empty());
        for c in &with.candidates {
            assert!(
                without
                    .candidates
                    .iter()
                    .any(|w| w.grow == c.grow && w.manual == c.manual && w.random == c.random),
                "帶入手能力後冒出原本沒有的解：{c:?}"
            );
        }
        assert!(
            with.candidates.len() < without.candidates.len(),
            "入手能力一組解都沒砍掉（{} → {}），這個條件等於沒接上",
            without.candidates.len(),
            with.candidates.len()
        );
    }

    /// 入手能力對不上時要乾脆地回無解，而不是默默忽略。
    #[test]
    fn wrong_catch_stats_rule_everything_out() {
        let (grow, target, catch, _, _, opts) = wild_case();
        let bogus = Target::new(catch.lvl, 9999, 9999, 9999, 9999, 9999);
        let o = solve(
            grow,
            target,
            GuessOptions {
                catch: Some(bogus),
                ..opts
            },
        );
        assert!(
            o.candidates.is_empty(),
            "亂填的入手能力竟然還有 {} 組解",
            o.candidates.len()
        );
    }

    /// 入手能力比對的是**實際**檔次，不是圖鑑檔次 —— 掉過檔的寵也要推得出來。
    #[test]
    fn catch_stats_are_matched_against_the_actual_tier() {
        let catalog = GrowRange::new(30, 25, 18, 20, 22, DEFAULT_BPRATE);
        let actual = GrowRange::new(28, 25, 16, 20, 21, DEFAULT_BPRATE); // 體 −2 防 −2 魔 −1
        let random = [2, 2, 2, 2, 2];
        let manual = [5, 5, 3, 3, 3]; // 共 19 點 ＝ 30 - 1 - 10

        let now = stat_at(actual, 30, manual, random);
        let then = stat_at(actual, 11, [0; AXES], random);

        let o = solve(
            catalog,
            Target::new(30, now.hp, now.mp, now.atk, now.def, now.agi),
            GuessOptions {
                not_order_point: 10,
                random: RandomSearch::Exhaustive,
                catch: Some(Target::new(
                    11, then.hp, then.mp, then.atk, then.def, then.agi,
                )),
                ..Default::default()
            },
        );
        assert!(!o.candidates.is_empty(), "掉檔的野寵帶入手能力就推不出來了");
        assert!(
            o.candidates
                .iter()
                .any(|c| c.grow == actual && c.random == random),
            "解裡沒有實際檔次 {:?}",
            actual.to_array()
        );
    }

    /// 觀測值亂填時應該是無解，而不是 panic 或吐出垃圾。
    #[test]
    fn impossible_target_yields_no_candidates() {
        let grow = GrowRange::new(10, 10, 10, 10, 10, DEFAULT_BPRATE);
        let target = Target::new(5, 99999, 99999, 99999, 99999, 99999);
        let o = solve(grow, target, GuessOptions::default());
        assert!(o.candidates.is_empty());
    }
}
