//! 前端 ↔ 計算核心的資料形狀。
//!
//! `petcalc` / `petdata` 是純計算 crate，刻意不依賴 serde，所以序列化的形狀
//! 全部集中在這裡。順帶把兩種軸順序的轉換也收斂到這一個檔案：
//!
//! * **BP 軸順序** `[HP, ATK, DEF, AGI, MP]` —— 檔次、加點、隨機檔都用這個
//! * **能力顯示順序** 血魔攻防敏（精神回復）—— 只有 [`StatDto`] 用這個
//!
//! 弄混這兩個順序是這個專案最容易踩的坑，所以轉換只發生在本檔案裡。

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use petcalc::{
    Bp, Candidate, Distribution, GrowRange, GuessOptions, LossBounds, ManualBounds, MatchMode,
    RandomBounds, RandomSearch, Stat, Target, AXES, DEFAULT_BPRATE, MAX_LOSS_BOUND,
    MAX_LOST_PER_AXIS, RANDOM_POOL,
};
use petdata::{race_name, Catalog, Pet};

/// 隨機檔的預設值 —— 對應原程式側欄「隨機檔 [2][2][2][2][2]」的初始狀態。
pub const DEFAULT_RANDOM: [i32; AXES] = [2, 2, 2, 2, 2];

// ── 圖鑑 ────────────────────────────────────────────────────────────────────

/// 一隻圖鑑寵物。
///
/// 雙向：送出去給清單顯示，也收回來當「保存」的輸入。三個標了
/// `skip_deserializing` 的欄位是**送出去才有意義**的衍生值 ——
/// 前端把整個物件原樣送回來時它們會被忽略，由後端重算，
/// 這樣前端改壞了 `race_name` 或 `grow_sum` 也污染不到資料。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PetDto {
    /// 內建表有 483 筆沒有編號，所以可能是 `null`。前端要選寵物請用陣列索引當鍵。
    #[serde(default)]
    pub id: Option<i32>,
    pub name: String,
    pub race: u8,
    #[serde(skip_deserializing)]
    pub race_name: &'static str,
    /// `[HP, ATK, DEF, AGI, MP]`
    pub grow: [i32; AXES],
    #[serde(skip_deserializing)]
    pub grow_sum: i32,
    #[serde(default = "default_bprate")]
    pub bprate: f64,
    #[serde(default)]
    pub skills: Option<i32>,
    /// 地水火風，四項總和 100。內建表沒有這欄。
    #[serde(default)]
    pub element: Option<[i32; 4]>,
    /// 使用者自己加／改的 —— 決定了「刪除」能不能按。
    #[serde(skip_deserializing)]
    pub custom: bool,
}

impl From<&Pet> for PetDto {
    fn from(p: &Pet) -> Self {
        Self {
            id: p.id,
            name: p.name.clone(),
            race: p.race,
            race_name: race_name(p.race),
            grow: p.grow,
            grow_sum: p.grow.iter().sum(),
            bprate: p.bprate,
            skills: p.skills,
            element: p.element,
            custom: p.custom,
        }
    }
}

impl From<PetDto> for Pet {
    fn from(p: PetDto) -> Self {
        Pet {
            id: p.id,
            name: p.name.trim().to_string(),
            race: p.race,
            grow: p.grow,
            bprate: p.bprate,
            skills: p.skills,
            element: p.element,
            // 從前端存回來的一定是自製的 —— 上游圖鑑不接受寫入
            custom: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RaceDto {
    pub code: u8,
    pub name: &'static str,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogDto {
    /// 資料來源描述（`builtin` 或載入的檔名）。
    pub source: String,
    pub count: usize,
    pub pets: Vec<PetDto>,
    /// 依原程式寵物檔案視窗的分頁順序排好的種族清單。
    pub races: Vec<RaceDto>,
}

impl From<&Catalog> for CatalogDto {
    fn from(c: &Catalog) -> Self {
        let races = petdata::RACE_TAB_ORDER
            .iter()
            .map(|&code| RaceDto {
                code,
                name: race_name(code),
                count: c.pets.iter().filter(|p| p.race == code).count(),
            })
            .collect();

        Self {
            source: c.source.clone(),
            count: c.pets.len(),
            pets: c.pets.iter().map(PetDto::from).collect(),
            races,
        }
    }
}

// ── 檔次／能力 ──────────────────────────────────────────────────────────────

/// 五軸檔次。欄位名與 [`GrowRange`] 一致，軸順序 `[HP, ATK, DEF, AGI, MP]`。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GrowDto {
    pub hp: i32,
    pub atk: i32,
    pub def: i32,
    pub agi: i32,
    pub mp: i32,
    #[serde(default = "default_bprate")]
    pub bprate: f64,
}

fn default_bprate() -> f64 {
    DEFAULT_BPRATE
}

impl From<GrowDto> for GrowRange {
    fn from(g: GrowDto) -> Self {
        GrowRange::new(g.hp, g.atk, g.def, g.agi, g.mp, g.bprate)
    }
}

impl From<GrowRange> for GrowDto {
    fn from(g: GrowRange) -> Self {
        Self {
            hp: g.hp,
            atk: g.atk,
            def: g.def,
            agi: g.agi,
            mp: g.mp,
            bprate: g.bprate,
        }
    }
}

/// 能力值，顯示順序 血魔攻防敏（精神回復）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct StatDto {
    pub hp: i64,
    pub mp: i64,
    pub atk: i64,
    pub def: i64,
    pub agi: i64,
    pub wis: i64,
    pub res: i64,
}

impl From<Stat> for StatDto {
    fn from(s: Stat) -> Self {
        Self {
            hp: s.hp,
            mp: s.mp,
            atk: s.atk,
            def: s.def,
            agi: s.agi,
            wis: s.wis,
            res: s.res,
        }
    }
}

/// 觀測到的能力（推算的輸入）。精神／回復不參與推算，所以這裡沒有。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TargetDto {
    pub lvl: i32,
    pub hp: i64,
    pub mp: i64,
    pub atk: i64,
    pub def: i64,
    pub agi: i64,
}

impl From<TargetDto> for Target {
    fn from(t: TargetDto) -> Self {
        Target::new(t.lvl, t.hp, t.mp, t.atk, t.def, t.agi)
    }
}

// ── 正向計算 ────────────────────────────────────────────────────────────────

/// 加點方式。對應原程式的「運算模式」（§4.2）。
///
/// 智／野是**推算**模式而不是正向模式（等級點的去向未知），所以不在這裡；
/// 正向計算要嘛不加點、要嘛固定加某一軸、要嘛給一個比例。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PointMode {
    /// 無 —— 等級點完全沒配。
    #[default]
    None,
    Hp,
    Atk,
    Def,
    Agi,
    Mp,
    /// 依 `ratio` 的比例分配（混加）。
    Ratio,
}

impl PointMode {
    /// 把 `total` 點依模式分配到五軸。
    ///
    /// `Ratio` 用最大餘額法，保證分完後總和剛好等於 `total`（不會因為取整少一點）。
    pub fn allocate(self, total: i32, ratio: [i32; AXES]) -> [i32; AXES] {
        if total <= 0 {
            return [0; AXES];
        }
        let axis = match self {
            PointMode::None => return [0; AXES],
            PointMode::Hp => 0,
            PointMode::Atk => 1,
            PointMode::Def => 2,
            PointMode::Agi => 3,
            PointMode::Mp => 4,
            PointMode::Ratio => return largest_remainder(total, ratio),
        };
        let mut out = [0; AXES];
        out[axis] = total;
        out
    }
}

/// 最大餘額法：按 `weights` 的比例把 `total` 分成整數，總和精確等於 `total`。
fn largest_remainder(total: i32, weights: [i32; AXES]) -> [i32; AXES] {
    let sum: i32 = weights.iter().map(|w| (*w).max(0)).sum();
    if sum <= 0 {
        return [0; AXES];
    }
    let mut out = [0i32; AXES];
    let mut rema = [(0.0f64, 0usize); AXES];
    let mut assigned = 0;
    for i in 0..AXES {
        let exact = f64::from(total) * f64::from(weights[i].max(0)) / f64::from(sum);
        out[i] = exact.floor() as i32;
        assigned += out[i];
        rema[i] = (exact - exact.floor(), i);
    }
    // 餘數大的先拿，同餘數時軸序在前的先拿（結果可重現）
    rema.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap().then(a.1.cmp(&b.1)));
    for &(_, i) in rema.iter().take((total - assigned).max(0) as usize) {
        out[i] += 1;
    }
    out
}

#[derive(Debug, Clone, Deserialize)]
pub struct ForwardReq {
    pub grow: GrowDto,
    pub lvl: i32,
    /// 隨機檔，總和應為 10。省略時用 [`DEFAULT_RANDOM`]。
    #[serde(default)]
    pub random: Option<[i32; AXES]>,
    #[serde(default)]
    pub mode: PointMode,
    /// [`PointMode::Ratio`] 的權重。
    #[serde(default)]
    pub ratio: Option<[i32; AXES]>,
    /// 直接指定加點，指定後就忽略 `mode` / `ratio`。
    #[serde(default)]
    pub manual: Option<[i32; AXES]>,
    /// 未分配的等級點（野寵入手等級的那一段）。
    #[serde(default)]
    pub not_order_point: i32,
    /// 「暴點加點」：從 `lvl` **之後**的等級改把點配到這一軸。
    /// `None` ＝ 下拉選單的「下次」＝ 整段都照 `mode` 走。
    ///
    /// ⚠️ **這是重新定義過的語意，不是原程式行為的複刻。**
    /// 原程式的「暴點加點」下拉在反編譯裡是**只寫不讀**的：
    /// 原程式把選中的索引存起來之後就沒有任何地方讀它 ——
    /// 沒有分支、沒有算式用到它。
    /// 也就是說它在原程式裡不影響任何計算，真正的意圖已經無從還原。
    ///
    /// 與其擺一個按了沒反應的控制項，這裡給它一個明確、有用、而且只用到
    /// 既有正算路徑的意思：「從下一級起改加這一軸」。搭配 `mode`（過去怎麼加）
    /// 就能回答「我一直加力量，之後改加體力，100 級會長什麼樣」——
    /// 這正是「模擬」分頁的成長表要的東西。
    #[serde(default)]
    pub burst: Option<PointMode>,
}

impl ForwardReq {
    /// 這一組參數在 `lvl` 級時的加點分布。
    ///
    /// 沒設 `burst` 時就是「`total` 點全照 `mode` 配」。設了的話拆成兩段：
    /// 到 `self.lvl` 為止的點照 `mode`，之後多出來的點改配到 `burst` 那一軸。
    pub fn manual_at(&self, lvl: i32) -> [i32; AXES] {
        if let Some(m) = self.manual {
            return m;
        }
        let ratio = self.ratio.unwrap_or([1; AXES]);
        let total = self.point_pool(lvl);
        let Some(burst) = self.burst else {
            return self.mode.allocate(total, ratio);
        };
        // 已經配掉的那一段（＝當前等級為止）。查詢等級比當前等級低時
        // `total` 就是全部，min 讓它退化成單純的 mode 分配。
        let before = self.point_pool(self.lvl).min(total);
        let past = self.mode.allocate(before, ratio);
        let future = burst.allocate(total - before, ratio);
        std::array::from_fn(|i| past[i] + future[i])
    }

    /// `lvl` 級時可分配的等級點數。
    fn point_pool(&self, lvl: i32) -> i32 {
        (lvl - 1 - self.not_order_point).max(0)
    }

    pub fn random_or_default(&self) -> [i32; AXES] {
        self.random.unwrap_or(DEFAULT_RANDOM)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ForwardRow {
    pub lvl: i32,
    /// `[HP, ATK, DEF, AGI, MP]`
    pub bp: [f64; AXES],
    pub bp_sum: f64,
    pub manual: [i32; AXES],
    pub stat: StatDto,
}

/// 組出某等級的完整 BP：基礎成長 ＋ 隨機檔 ＋ 加點。
pub fn compose_bp(grow: GrowRange, lvl: i32, random: [i32; AXES], manual: [i32; AXES]) -> Bp {
    let base = grow.calc_bp_at_level(lvl, None).base_bp.to_array();
    Bp::from_array(std::array::from_fn(|i| {
        base[i] + grow.bprate * f64::from(random[i]) + f64::from(manual[i])
    }))
}

pub fn forward_row(req: &ForwardReq, lvl: i32) -> ForwardRow {
    let grow: GrowRange = req.grow.into();
    let manual = req.manual_at(lvl);
    let bp = compose_bp(grow, lvl, req.random_or_default(), manual);
    ForwardRow {
        lvl,
        bp: bp.to_array(),
        bp_sum: bp.sum(),
        manual,
        stat: bp.calc_real_num().into(),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeriesReq {
    #[serde(flatten)]
    pub base: ForwardReq,
    pub from: i32,
    pub to: i32,
}

// ── 推算 ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MatchModeDto {
    /// 由緊到鬆依序試 `exact` → `observer` → `tolerant`。
    Auto,
    Exact,
    /// 原程式的規則：只在檔次落在成長係數表五週期不規則處才放寬。
    ///
    /// **這是預設值** —— 這個專案要的是「跟原程式一樣」，
    /// 沒指定模式時就該走原程式那條，而不是 cg-pet-calc 的精確列舉。
    #[default]
    Observer,
    /// 全軸一律放寬。移植版自己的延伸，不是原程式的行為。
    Tolerant,
}

impl From<MatchModeDto> for MatchMode {
    fn from(m: MatchModeDto) -> Self {
        match m {
            MatchModeDto::Auto => MatchMode::Auto,
            MatchModeDto::Exact => MatchMode::Exact,
            MatchModeDto::Observer => MatchMode::Observer,
            MatchModeDto::Tolerant => MatchMode::Tolerant,
        }
    }
}

pub fn match_mode_name(m: MatchMode) -> &'static str {
    match m {
        MatchMode::Auto => "auto",
        MatchMode::Exact => "exact",
        MatchMode::Observer => "observer",
        MatchMode::Tolerant => "tolerant",
    }
}

// ── 運算模式 ────────────────────────────────────────────────────────────────

/// 主視窗的「運算模式」八選一（CLAUDE.md §4.2）。
///
/// 這八個鍵其實在講兩件不同的事：後面六個直接限死了加點怎麼配，
/// 前兩個則把加點交給側欄的「加點方式」決定 —— 見 [`CalcMode::takes_manual_plan`]。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CalcMode {
    /// 智 —— 家寵，等級點自由分配。
    #[default]
    Smart,
    /// 野 —— 野寵，等級點由系統隨機配。
    ///
    /// 解空間跟「智」一樣（都是自由分配），差別在前端：野寵沒有玩家加點，
    /// 取而代之的是「入手等級」與「入手能力」欄位。
    Wild,
    /// 無 —— 完全未加點，`lvl - 1` 點全部沒配。
    None,
    Hp,
    Atk,
    Def,
    Agi,
    Mp,
}

impl CalcMode {
    /// 這個模式本身隱含的加點限制。
    pub fn bounds(self) -> ManualBounds {
        match self {
            CalcMode::Smart | CalcMode::Wild => ManualBounds::free(),
            // 「無」的點是**沒配掉**，不是「配了 0 點」——
            // 真正的表達方式是 not_order_point，見 `GuessReq::effective_not_order_point`。
            CalcMode::None => ManualBounds::zero(),
            CalcMode::Hp => ManualBounds::single(0),
            CalcMode::Atk => ManualBounds::single(1),
            CalcMode::Def => ManualBounds::single(2),
            CalcMode::Agi => ManualBounds::single(3),
            CalcMode::Mp => ManualBounds::single(4),
        }
    }

    /// 只有「智／野」會去看側欄的加點方式；其餘六種模式自己就把加點釘死了。
    pub fn takes_manual_plan(self) -> bool {
        matches!(self, CalcMode::Smart | CalcMode::Wild)
    }
}

/// 側欄的「加點方式」（CLAUDE.md §4.3）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase", tag = "kind")]
pub enum ManualPlanDto {
    /// 不限制。
    #[default]
    Free,
    /// 指定加點：五軸各釘死一個數。
    Fixed { points: [i32; AXES] },
    /// 混加（只勾種類）：沒勾的軸不准加點，勾的軸不限量。
    Mixed { axes: [bool; AXES] },
    /// 混加（連範圍一起給）：逐軸 `[下界, 上界]`。
    Range { bounds: [[i32; 2]; AXES] },
}

impl From<ManualPlanDto> for ManualBounds {
    fn from(p: ManualPlanDto) -> Self {
        match p {
            ManualPlanDto::Free => ManualBounds::free(),
            ManualPlanDto::Fixed { points } => ManualBounds::fixed(points),
            ManualPlanDto::Mixed { axes } => ManualBounds::mixed(axes),
            ManualPlanDto::Range { bounds } => ManualBounds::new(bounds),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GuessReq {
    /// 圖鑑檔次（上限）。
    pub grow: GrowDto,
    pub target: TargetDto,
    /// 已知未分配的等級點。野寵的「入手等級 N」＝ `N - 1` 點是系統配的，
    /// 那段仍然配掉了，所以這裡通常還是 0；真正沒配才填。
    #[serde(default)]
    pub not_order_point: i32,
    /// 運算模式（智／野／無／體／力／防／敏／魔）。
    #[serde(default)]
    pub calc_mode: CalcMode,
    /// 側欄的加點方式；只在「智／野」下有意義。
    #[serde(default)]
    pub manual: Option<ManualPlanDto>,
    /// 側欄「使用入手能力」填的入手觀測；`lvl` 就是「入手等級」。
    ///
    /// 入手當下玩家還沒加點，所以這串能力只由實際檔次與隨機檔決定 ——
    /// 是一條比當前能力更硬的約束，見 [`GuessOptions::catch`]。
    #[serde(default)]
    pub catch_stat: Option<TargetDto>,
    #[serde(default)]
    pub mode: MatchModeDto,
    /// 隨機檔窮舉全部 1001 組（慢但完備），否則只搜代數估計的 ±1 鄰域。
    #[serde(default)]
    pub exhaustive: bool,
    #[serde(default)]
    pub tolerance: Option<f64>,
    /// 指定實際檔次，跳過 3125 組列舉。
    #[serde(default)]
    pub target_grow: Option<GrowDto>,
    /// 引擎端的解數上限（0＝不設限）。這是**搜尋**上限，跟回傳筆數上限不同。
    #[serde(default)]
    pub limit: usize,
    /// 推算的搜尋範圍（原程式的「檔次範圍」「隨機檔範圍」）。
    ///
    /// 省略時取 [`RangeLimitsDto::default`]，也就是原程式那兩欄的預設值。
    #[serde(default)]
    pub ranges: RangeLimitsDto,
}

/// 推算的搜尋範圍 —— 對應原程式那一頁的「檔次範圍」與「隨機檔範圍」。
///
/// 原程式的欄位是兩串 `上限;下限` 的字串，預設值分別是
/// `4 4 4 4 4;0 0 0 0 0` 與 `0 0 0 0 0;10 10 10 10 10`。這裡拆成四個陣列，
/// [`Default`] 就是那組預設值 —— 所以前端不傳這個欄位時，搜尋範圍與
/// 加這個功能之前**完全一樣**。
///
/// 面板上那句提示照原程式的意思保留在介面：這是給知道自己在做什麼的人
/// 縮小或放寬解空間用的，亂調只會得到錯的解或等很久。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct RangeLimitsDto {
    /// 逐軸**最少**掉幾檔。
    pub loss_min: [i32; AXES],
    /// 逐軸**最多**掉幾檔。
    pub loss_max: [i32; AXES],
    /// 逐軸隨機檔下界。
    pub random_min: [i32; AXES],
    /// 逐軸隨機檔上界。
    pub random_max: [i32; AXES],
}

impl Default for RangeLimitsDto {
    fn default() -> Self {
        Self {
            loss_min: [0; AXES],
            loss_max: [MAX_LOST_PER_AXIS; AXES],
            random_min: [0; AXES],
            random_max: [RANDOM_POOL; AXES],
        }
    }
}

impl RangeLimitsDto {
    pub fn loss(&self) -> LossBounds {
        LossBounds::new(self.loss_min, self.loss_max)
    }

    pub fn random(&self) -> RandomBounds {
        RandomBounds::new(self.random_min, self.random_max)
    }
}

impl GuessReq {
    /// 運算模式挑到的加點限制，側欄的加點方式優先（僅限「智／野」）。
    pub fn manual_bounds(&self) -> ManualBounds {
        match self.manual {
            Some(plan) if self.calc_mode.takes_manual_plan() => plan.into(),
            _ => self.calc_mode.bounds(),
        }
    }

    /// 入手等級必須落在 `1..=當前等級`。
    ///
    /// 這種填錯不能默默忽略掉條件：入手能力對不上時解會被濾光，使用者只會看到
    /// 「推不出解」，卻不知道問題其實出在入手等級填得比當前等級還高。
    pub fn check_catch(&self) -> Result<(), String> {
        match self.catch_stat {
            Some(c) if !(1..=self.target.lvl).contains(&c.lvl) => Err(format!(
                "入手等級 {} 必須在 1..={} 之間",
                c.lvl, self.target.lvl
            )),
            _ => Ok(()),
        }
    }

    /// 隨機檔範圍必須湊得出總和 10。
    ///
    /// 跟 [`check_catch`](Self::check_catch) 同樣的理由：條件矛盾與真的推不出解
    /// 在畫面上長得一模一樣，但前者是使用者自己把範圍填死了 —— 要講出來，
    /// 不然他會以為是這隻寵物沒有解。
    pub fn check_ranges(&self) -> Result<(), String> {
        if self.ranges.random().is_satisfiable() {
            return Ok(());
        }
        let lo: i32 = self.ranges.random_min.iter().sum();
        let hi: i32 = self.ranges.random_max.iter().sum();
        Err(format!(
            "隨機檔範圍湊不出總和 {RANDOM_POOL}：下界合計 {lo}、上界合計 {hi}"
        ))
    }

    /// 「無」的語意是那 `lvl - 1` 點**根本沒配掉**，不是配了 0 點 ——
    /// 只給 `ManualBounds::zero()` 的話，點數對不上會直接搜不到解。
    pub fn effective_not_order_point(&self) -> i32 {
        if self.calc_mode == CalcMode::None {
            (self.target.lvl - 1).max(self.not_order_point)
        } else {
            self.not_order_point
        }
    }
}

impl From<&GuessReq> for GuessOptions {
    fn from(r: &GuessReq) -> Self {
        GuessOptions {
            not_order_point: r.effective_not_order_point(),
            manual: r.manual_bounds(),
            catch: r.catch_stat.map(Into::into),
            target_grow: r.target_grow.map(Into::into),
            mode: r.mode.into(),
            random: if r.exhaustive {
                RandomSearch::Exhaustive
            } else {
                RandomSearch::Ball { radius: 1 }
            },
            limit: r.limit,
            tolerance: r.tolerance,
            loss: r.ranges.loss(),
            random_bounds: r.ranges.random(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CandidateDto {
    /// 推出來的實際檔次 `[HP, ATK, DEF, AGI, MP]`。
    pub grow: [i32; AXES],
    /// 逐軸掉檔量＝圖鑑 − 實際。
    pub lost: [i32; AXES],
    pub lost_bp: i32,
    pub manual: [i32; AXES],
    pub random: [i32; AXES],
    pub bp: [f64; AXES],
    pub stat: StatDto,
    /// 靠容差才成立（`exact` 模式恆為 false）。
    pub approximate: bool,
    /// 這組解的機率（百分點）。
    pub percent: f64,
}

fn candidate_dto(c: &Candidate, catalog: GrowRange, percent: f64) -> CandidateDto {
    let cat = catalog.to_array();
    let grow = c.grow.to_array();
    CandidateDto {
        grow,
        lost: std::array::from_fn(|i| cat[i] - grow[i]),
        lost_bp: c.lost_bp,
        manual: c.manual,
        random: c.random,
        bp: c.bp.to_array(),
        stat: c.stat.into(),
        approximate: c.approximate,
        percent,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DistributionDto {
    pub total_weight: u64,
    pub max_percent: f64,
    /// `[軸][掉檔量 0..=4]` 的邊際機率。
    pub lost_marginal: Vec<Vec<f64>>,
    /// `[軸][隨機檔 0..=10]` 的邊際機率。
    pub random_marginal: Vec<Vec<f64>>,
    /// `[總掉檔量 0..=20]` 的機率。
    ///
    /// **不能由 `lost_marginal` 推出來** —— 逐軸邊際丟掉了軸之間的相關性，
    /// 各軸最小值的和只是下界，未必真的湊得出來。見 `petcalc::stats`。
    pub lost_total_marginal: Vec<f64>,
    /// 總掉檔量的可能範圍 `[最小, 最大]`，沒有候選時是 `null`。
    ///
    /// 對全集統計，**不受 `candidates` 那 200 筆上限影響**。
    pub lost_total_range: Option<[i32; 2]>,
    /// 已經唯一確定的軸（機率 ≈100% 那一格），未確定的是 `null`。
    pub determined_lost: Vec<Option<i32>>,
    pub fully_determined: bool,
    /// ⭐ **穩掉** —— 每軸「一定至少掉了幾檔」。原程式主視窗結果欄第一行就印它
    /// （`穩掉：2體 4防 3魔`，掉 0 檔的不列）。
    ///
    /// 比 `determined_lost` 寬：那個只認「整條只剩一格」的軸，
    /// 所以某軸可能掉 2 或 3 檔時它什麼都不說，但「至少 2 檔」是確定的。
    pub guaranteed_lost: Vec<i32>,
    /// 相異的掉檔組合與各自的機率總和，照總掉檔遞增排序。
    ///
    /// **對全集統計，不受 `candidates` 那 200 筆上限影響。**
    /// 也**不能由 `lost_marginal` 推出來** —— 逐軸邊際沒有保留軸之間的搭配。
    pub lost_combos: Vec<LostComboDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LostComboDto {
    /// 逐軸掉檔量 `[HP, ATK, DEF, AGI, MP]`，正數。
    pub lost: [i32; AXES],
    /// 這一組的總掉檔量。
    pub total: i32,
    /// 這一組的機率總和（百分點）。全部加起來是 100。
    pub percent: f64,
}

impl From<&Distribution> for DistributionDto {
    fn from(d: &Distribution) -> Self {
        Self {
            total_weight: d.total_weight,
            max_percent: d.max_percent,
            lost_marginal: d.lost_marginal.iter().map(|r| r.to_vec()).collect(),
            random_marginal: d.random_marginal.iter().map(|r| r.to_vec()).collect(),
            lost_total_marginal: d.lost_total_marginal.to_vec(),
            lost_total_range: d.lost_total_range().map(|(lo, hi)| [lo, hi]),
            determined_lost: d.determined_lost().to_vec(),
            fully_determined: d.is_fully_determined(),
            guaranteed_lost: d.guaranteed_lost().to_vec(),
            lost_combos: d
                .lost_combos
                .iter()
                .map(|c| LostComboDto {
                    lost: c.lost,
                    total: c.total,
                    percent: c.percent,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GuessResp {
    /// 實際生效的比對模式（`auto` 會具體化成 `exact` 或 `tolerant`）。
    pub mode: &'static str,
    /// 實際使用的加點總數。
    pub point: i32,
    /// 找到的解總數。
    pub total: usize,
    /// `candidates` 是否被截斷（只回傳機率最高的前幾筆）。
    pub truncated: bool,
    /// 依機率由高到低排序。
    pub candidates: Vec<CandidateDto>,
    pub distribution: Option<DistributionDto>,
}

/// 把引擎輸出整理成前端要的形狀：算機率 → 依機率排序 → 截斷。
///
/// 一併回傳 [`Distribution`]：`candidates` 會被截成 `max_rows` 筆，但機率查詢
/// 必須用**全部**候選才算得對，所以呼叫端要把它連同
/// `outcome.candidates` 一起留下來。
pub fn build_guess_resp(
    catalog: GrowRange,
    outcome: &petcalc::GuessOutcome,
    max_rows: usize,
) -> (GuessResp, Option<Distribution>) {
    let total = outcome.candidates.len();
    if total == 0 {
        let resp = GuessResp {
            mode: match_mode_name(outcome.mode),
            point: outcome.point,
            total: 0,
            truncated: false,
            candidates: Vec::new(),
            distribution: None,
        };
        return (resp, None);
    }

    let dist = Distribution::summarize(catalog, &outcome.candidates);

    let mut order: Vec<usize> = (0..total).collect();
    order.sort_by(|&a, &b| {
        dist.percent[b]
            .partial_cmp(&dist.percent[a])
            .unwrap_or(std::cmp::Ordering::Equal)
            // 同機率時用掉檔少的優先，再用索引穩定排序
            .then(
                outcome.candidates[a]
                    .lost_bp
                    .cmp(&outcome.candidates[b].lost_bp),
            )
            .then(a.cmp(&b))
    });

    let truncated = max_rows > 0 && total > max_rows;
    if truncated {
        order.truncate(max_rows);
    }

    let resp = GuessResp {
        mode: match_mode_name(outcome.mode),
        point: outcome.point,
        total,
        truncated,
        candidates: order
            .iter()
            .map(|&i| candidate_dto(&outcome.candidates[i], catalog, dist.percent[i]))
            .collect(),
        distribution: Some((&dist).into()),
    };
    (resp, Some(dist))
}

// ── 機率查詢 ───────────────────────────────────────────────────────────────

/// 機率查詢的輸入：五組檔次上下限 ＋ 一組「某幾軸合計 ≥ 門檻」。
///
/// 原程式是三個座標選擇器（可去重成 −1 停用）＋ 五組 `(min,max)`；這裡把
/// 軸選擇放寬成任意子集合，語意一樣但不必複製那套對照表（`CLAUDE.md` §3.5）。
#[derive(Debug, Clone, Deserialize)]
pub struct ProbabilityReq {
    pub lo: [i32; AXES],
    pub hi: [i32; AXES],
    /// 要合計的軸索引；空的話不算第二個輸出。
    #[serde(default)]
    pub axes: Vec<usize>,
    #[serde(default)]
    pub threshold: i32,
}

/// 機率查詢的兩個輸出，單位都是百分點。
#[derive(Debug, Clone, Serialize)]
pub struct ProbabilityResp {
    /// 五項檔次全部落在範圍內的機率。
    pub in_range: f64,
    /// 指定軸的檔次合計 ≥ 門檻的機率；沒選軸時為 `null`。
    pub axis_sum: Option<f64>,
    /// 這次統計用掉的候選筆數 —— 是全部，不是回傳列表被截斷後的那些。
    pub candidates: usize,
}

impl ProbabilityReq {
    /// 對一批候選求值。`dist` 必須是同一批候選算出來的分布。
    pub fn eval(&self, candidates: &[Candidate], dist: &Distribution) -> ProbabilityResp {
        let axes: Vec<usize> = self.axes.iter().copied().filter(|&i| i < AXES).collect();
        ProbabilityResp {
            in_range: dist.probability_in_grow_range(candidates, self.lo, self.hi),
            axis_sum: (!axes.is_empty())
                .then(|| dist.probability_axis_sum_at_least(candidates, &axes, self.threshold)),
            candidates: candidates.len(),
        }
    }
}

// ── 推算後的再篩選（原程式的「輸入更多資訊」）──────────────────────────────

/// 12 個篩選框，`null` ＝ 留空（原程式存 `-1`）。
///
/// ## 這是原程式的第二段，不是移植版發明的
///
/// 原程式推算完不會就結束：它把候選表留著，然後**在同一批候選上再掃一遍**，
/// 拿你補充的資訊把解砍掉。每動一格就重掃一次，某一欄在所有倖存候選裡
/// 只剩一個值時，那格連同它的標籤會被**隱藏**（沒必要再問），全部問完就收工。
///
/// ## ⭐ 原程式問的是哪 12 欄
///
/// 這裡先前寫的是「7 欄能力 ＋ 5 欄檔次」，**錯了**。原程式那 12 個標籤的
/// 文字是編在執行檔裡的常數，逐一解出來是：
///
/// | 欄     | 1    | 2    | 3–7                       | 8–12                      |
/// | ------ | ---- | ---- | ------------------------- | ------------------------- |
/// | 標籤   | 精神 | 回復 | 體力 力量 強度 速度 魔法  | 體力 力量 強度 速度 魔法  |
/// | 是什麼 | 能力 | 能力 | **逐軸 BP**               | **檔次**                  |
///
/// 後面兩組標籤一樣，靠**比對走哪條路**分辨。原程式的篩選是兩個迴圈：
/// 前 7 格一圈、後 5 格一圈，候選那邊各對到一段連續的 `double`。
///
/// | 欄   | 比對                                                                |
/// | ---- | ------------------------------------------------------------------- |
/// | 1–7  | `Trunc(候選值) == 你填的整數`，**沒有容差**                         |
/// | 8–12 | 同上，但不中時再套檔次專用的那段 ±容差（`檔次 mod 5 ∈ {0,1}`，§3.4）|
///
/// 容差那條路只有檔次會用（§3.4 講的就是檔次），所以 8–12 是檔次；
/// 剩下的 3–7 掛在「體力…魔法」五個標籤下、又不是檔次，只剩逐軸 BP。
/// 佐證是判斷「這一欄是否已確定」時多出來的那條後路（值的十進位寫法超過
/// 8 個字元就改比兩位小數）—— 精神／回復是整數、檔次幾乎是整數，
/// **會長出那種寫法的只有 BP**。
///
/// ⚠️ 那條後路移植版**沒有照抄**，見 [`RefineSettled`]。
///
/// ⭐ **生命 魔力 攻擊 防禦 敏捷 五格根本不在問題裡。** 它們是推算的輸入，
/// 早就被釘死了，問了也篩不掉東西 —— 原程式從一開始就沒把它們列進來。
/// 移植版先前照著「7 欄能力」的誤解畫了那五格，還得在旁邊解釋它們為什麼
/// 沒有自動變灰（[`Candidate::stat`] 是**沒加偏移**那份 BP 反算的，
/// 容差命中的候選會差 1）。現在那五格拿掉了，那段解釋也跟著消失。
///
/// ## 容差在檔次這一欄沒有對應物
///
/// 原程式比對檔次時有一段 ±容差（§3.4），是因為它的候選表把檔次存成**浮點**
/// （由 BP 反算），`Trunc` 會在係數表五週期的不規則處差 1。
/// 移植版的 [`Candidate::grow`] 是列舉出來的**整數**，取整這一步根本不存在，
/// 所以這裡是整數相等 —— 不是把容差漏掉，是它沒有東西可以修。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RefineReq {
    /// 生命 魔力 攻擊 防禦 敏捷 精神 回復。
    ///
    /// ⚠️ 原程式只問**精神（index 5）與回復（index 6）**，前五格不在它的
    /// 12 欄裡（見型別說明）。七格照樣收下：這一層是純粹的過濾器，
    /// 少一個維度不會讓它變簡單，而砍掉欄位反而會讓既有的請求變成不合法。
    #[serde(default)]
    pub stat: [Option<i64>; petcalc::STATS],
    /// 逐軸 BP 的**整數部分**，軸順序 `[HP, ATK, DEF, AGI, MP]`。
    ///
    /// ⚠️ **是整數，不是小數。** 原程式那 12 個框存的都是 `int`（`-1` ＝ 沒填），
    /// 比對時把候選的值 `Trunc` 成 Int64 再比 —— BP 是這 12 欄裡唯一的小數，
    /// 所以這一格問的就是「BP 的整數部分是多少」。
    ///
    /// 遊戲的「寵物狀態」視窗直接印這五個數，使用者抄得到；而且 BP 把加點與
    /// 隨機檔分得很開，是這一排最有力的篩選條件。
    #[serde(default)]
    pub bp: [Option<i64>; AXES],
    /// 檔次，BP 軸順序 `[HP, ATK, DEF, AGI, MP]`。
    #[serde(default)]
    pub grow: [Option<i32>; AXES],
}

/// 每一欄的可選值，遞增排序。**這 12 格在原程式裡是下拉，不是輸入框。**
///
/// 依據：那 12 個控制項的值不是從文字解析出來的，而是查一張
/// 「第幾個選項 × 第幾欄」的 `int` 表 —— 控制項只交出「你選了第幾項」與
/// 「我是第幾欄」兩個編號。旁證是篩到 0 組時標籤變成「**選**錯」，
/// 以及「一開始就只有一個值的欄位根本不建控制項」這個行為 ——
/// 那需要程式先知道每一欄有哪些相異值。
///
/// 表的填充碼沒找到，但內容只有一種填法說得通：**倖存候選在該欄的相異值**。
/// 少列一個值，正確答案就選不到；多列一個值，選了必然篩到空。
///
/// ⚠️ **遞增排序是移植版挑的**，原程式的順序沒逆出來。
#[derive(Debug, Clone, Serialize)]
pub struct RefineOptions {
    pub stat: [Vec<i64>; petcalc::STATS],
    /// 逐軸 BP 的**整數部分**（`Trunc`）—— 篩選比的就是這個量。
    pub bp: [Vec<i64>; AXES],
    pub grow: [Vec<i32>; AXES],
}

/// 每一欄「所有倖存候選是否同一個值」—— 是的話原程式把那格藏起來。
///
/// 由 [`RefineOptions`] 導出（`只剩一個選項` ⇔ 已確定），不另外算一份，
/// 免得兩邊對同一件事講不一樣的話。
///
/// ## 跟篩選同一把尺：`Trunc`
///
/// 原程式判斷一致與否，主路就是比 `Trunc` 之後的整數 —— 跟 [`RefineReq`]
/// 那 12 個框比的是同一個東西。這件事讓面板收得了工：你在 BP 那格填了 `8`，
/// 倖存候選的 `Trunc` 就都是 8，那一欄隨即消失。
///
/// ## ⚠️ 有一條後路沒有照抄
///
/// 原程式在主路之外還有一條：把值格式化成字串，**寫法超過 8 個字元**時改拿
/// 「四捨五入到兩位小數」的字串去比。那是給浮點雜訊用的補丁，而且它比主路**嚴**
/// —— 於是 `93.835` 走主路得到 `93`，`93.83500000000001` 走後路得到 `93.84`，
/// 兩筆幾乎相同的值會被判成不一致。
///
/// 不照抄的理由跟 CLAUDE.md §6.1 最後一條是同一件事：原程式算在 80-bit
/// `Extended` 上，移植版是 `f64`，**雜訊出現在哪一筆本來就不一樣**。
/// 照抄等於照抄一個平台相依的隨機行為，不是照抄行為。
#[derive(Debug, Clone, Serialize)]
pub struct RefineSettled {
    pub stat: [Option<i64>; petcalc::STATS],
    /// 逐軸 BP 的**整數部分**（`Trunc`），跟 [`RefineReq::bp`] 同一把尺。
    pub bp: [Option<i64>; AXES],
    pub grow: [Option<i32>; AXES],
    /// **原程式問的那 12 欄**全部確定 —— 它在這一刻收工，不再追問。
    ///
    /// 精神／回復 ＋ BP 五軸 ＋ 檔次五軸。生命 魔力 攻擊 防禦 敏捷
    /// 不算在內：原程式沒問它們，而且移植版的容差候選讓它們可能永遠不一致
    /// （見 [`RefineReq`]），算進來的話這面板就永遠收不了工。
    pub all: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RefineResp {
    /// 篩完之後的結果，形狀跟 [`GuessResp`] 一模一樣 ——
    /// 前端可以拿同一組元件畫，不必為篩選結果另做一套。
    pub result: GuessResp,
    /// 篩之前的候選數，讓畫面能講「N 組裡剩 M 組」。
    pub before: usize,
    /// 倖存候選佔原本機率總量的百分比。
    ///
    /// ⚠️ `result` 裡的機率是**對倖存集重新正規化**過的（加起來 100%），
    /// 原程式則是留著原本的權重不重算。改成重算是為了讓篩選結果能沿用
    /// 同一組顯示元件（掉檔組合那張表加起來要是 100% 才看得懂），
    /// 原本那個「還剩多少」的資訊就搬到這個欄位，沒有弄丟。
    pub kept_percent: f64,
    /// 每一欄的下拉選項。前端只取**第一次（沒篩任何東西）**那份 ——
    /// 原程式也是推算完建一次控制項，不會因為後來篩窄了就冒出新問題。
    pub options: RefineOptions,
    pub settled: RefineSettled,
    /// 倖存候選的逐軸掉檔範圍 `[最小, 最大]`，沒有候選時是 `null`。
    ///
    /// ⭐ **原程式篩完就是這樣回頭改「搜尋範圍」的**：它掃一遍倖存候選累出
    /// 逐軸的檔次 min/max，換算成 `圖鑑 − min`（掉檔上限）與 `圖鑑 − max`
    /// （掉檔下限），組成 `上限五個;下限五個` 塞回那個輸入框（見 §4.3）。
    /// 下次按計算就會在收窄過的空間裡重推。
    ///
    /// ⚠️ **不能拿 [`DistributionDto::lost_marginal`] 算這個。** 那張表只有
    /// `0..=4` 五格，掉更多的候選會被**默默丟掉**（`petcalc::stats` 那裡寧可
    /// 漏統計也不 panic），拿它取頭尾會把範圍算窄。這裡掃的是完整的倖存集。
    pub narrowed_loss: Option<[[i32; 2]; AXES]>,
}

impl RefineReq {
    /// 掃一遍候選表，套 12 個框。留空的欄不比對。
    pub fn eval(
        &self,
        catalog: GrowRange,
        cache: &[Candidate],
        dist: &Distribution,
        mode: MatchMode,
        point: i32,
        max_rows: usize,
    ) -> RefineResp {
        let mut kept_percent = 0.0;
        let mut kept: Vec<Candidate> = Vec::new();
        for (i, c) in cache.iter().enumerate() {
            if self.keeps(c) {
                kept.push(*c);
                kept_percent += dist.percent[i];
            }
        }

        let options = RefineOptions::over(&kept);
        let settled = RefineSettled::from(&options);
        let narrowed_loss = narrowed_loss(catalog, &kept);
        let outcome = petcalc::GuessOutcome {
            candidates: kept,
            mode,
            point,
        };
        let (result, _) = build_guess_resp(catalog, &outcome, max_rows);
        RefineResp {
            result,
            before: cache.len(),
            kept_percent,
            options,
            settled,
            narrowed_loss,
        }
    }

    fn keeps(&self, c: &Candidate) -> bool {
        let stat = stat_columns(c.stat);
        if (0..petcalc::STATS).any(|i| matches!(self.stat[i], Some(w) if w != stat[i])) {
            return false;
        }
        let bp = c.bp.to_array();
        if (0..AXES).any(|i| matches!(self.bp[i], Some(w) if bp[i].trunc() as i64 != w)) {
            return false;
        }
        let grow = c.grow.to_array();
        !(0..AXES).any(|i| matches!(self.grow[i], Some(w) if w != grow[i]))
    }
}

/// 掃倖存候選，逐軸算出 `[最小掉檔, 最大掉檔]`。
///
/// 原程式是累 **檔次** 的 min/max 再用圖鑑換算，等價；這裡直接算掉檔，
/// 少一次轉換也少一次符號搞反的機會。
fn narrowed_loss(catalog: GrowRange, kept: &[Candidate]) -> Option<[[i32; 2]; AXES]> {
    if kept.is_empty() {
        return None;
    }
    let cat = catalog.to_array();
    Some(std::array::from_fn(|i| {
        let mut lo = i32::MAX;
        let mut hi = i32::MIN;
        for c in kept {
            let lost = cat[i] - c.grow.to_array()[i];
            lo = lo.min(lost);
            hi = hi.max(lost);
        }
        [lo, hi]
    }))
}

impl RefineOptions {
    fn over(kept: &[Candidate]) -> Self {
        // BP 收的是 `Trunc` 之後的整數 —— 選項要跟篩選比的量一致，
        // 不然畫面會列出兩個選項、選哪個都留下同一批候選。
        let mut stat: [BTreeSet<i64>; petcalc::STATS] = Default::default();
        let mut bp: [BTreeSet<i64>; AXES] = Default::default();
        let mut grow: [BTreeSet<i32>; AXES] = Default::default();
        for c in kept {
            let s = stat_columns(c.stat);
            for (i, set) in stat.iter_mut().enumerate() {
                set.insert(s[i]);
            }
            let b = c.bp.to_array();
            let g = c.grow.to_array();
            for i in 0..AXES {
                bp[i].insert(b[i].trunc() as i64);
                grow[i].insert(g[i]);
            }
        }
        Self {
            stat: stat.map(|s| s.into_iter().collect()),
            bp: bp.map(|s| s.into_iter().collect()),
            grow: grow.map(|s| s.into_iter().collect()),
        }
    }
}

impl From<&RefineOptions> for RefineSettled {
    /// 只剩一個選項 ＝ 那一欄已確定。一個選項都沒有（篩到空）就不是確定，
    /// 否則畫面會把「條件互斥」說成「全部問完了」。
    fn from(o: &RefineOptions) -> Self {
        let only = |v: &Vec<i64>| (v.len() == 1).then(|| v[0]);
        let stat = std::array::from_fn(|i| only(&o.stat[i]));
        let bp = std::array::from_fn(|i| only(&o.bp[i]));
        let grow = std::array::from_fn(|i| (o.grow[i].len() == 1).then(|| o.grow[i][0]));
        Self {
            // ⭐ 只數原程式問的那 12 欄 —— 精神／回復 ＋ BP 五軸 ＋ 檔次五軸。
            all: stat[5].is_some()
                && stat[6].is_some()
                && bp.iter().all(Option::is_some)
                && grow.iter().all(Option::is_some),
            stat,
            bp,
            grow,
        }
    }
}

/// 能力值攤成 12 格篩選器的前 7 欄，順序跟 [`StatDto`] 一致。
fn stat_columns(s: Stat) -> [i64; petcalc::STATS] {
    [s.hp, s.mp, s.atk, s.def, s.agi, s.wis, s.res]
}

// ── 寵物搜尋 ───────────────────────────────────────────────────────────────

/// 「搜索不低於輸入能力的寵物」的輸入。
///
/// 欄位對應原程式面板上的 10 個輸入框：技能、等級、七項能力、屬性。
/// 數學在 [`petcalc::search`]，這裡只做形狀轉換與圖鑑過濾。
#[derive(Debug, Clone, Deserialize)]
pub struct SearchReq {
    /// 目標等級。原程式要求 > 1，這裡沿用。
    pub lvl: i32,
    /// 血 魔 攻 防 敏 精神 回復 的下限，`0` ＝ 不限。
    pub floor: [i64; petcalc::STATS],
    /// 技能格數下限，`0` ＝ 不限（原程式用 `>=` 比）。
    #[serde(default)]
    pub skills: i32,
    /// 屬性：`0` ＝ 不限，`1..=4` ＝ 地／水／火／風該項必須 > 0。
    #[serde(default)]
    pub element: usize,
    /// 假設的隨機檔。原程式用哪組沒查出來，預設跟介面其他地方一致。
    #[serde(default)]
    pub random: Option<[i32; AXES]>,
    /// 最多回幾筆，`0` ＝ 不限。
    #[serde(default)]
    pub limit: usize,
}

/// 一筆命中的寵物。
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub pet: PetDto,
    /// 該寵在這個等級、完全不加點時的能力。
    pub stat: StatDto,
    /// 把每項補到下限所需的加點數合計（無條件進位到整數點）。
    pub needed: i32,
    /// 補完之後還剩幾點可以自由分配 —— 越多代表越有餘裕。
    pub spare: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResp {
    /// 可用的加點數 ＝ `lvl - 1`。
    pub available: i32,
    /// 命中總數（`hits` 可能被 `limit` 截斷）。
    pub total: usize,
    pub truncated: bool,
    /// 依剩餘點數由多到少排序 —— 最有餘裕的排前面。
    pub hits: Vec<SearchHit>,
}

impl SearchReq {
    /// 對一份圖鑑跑搜尋。
    pub fn run(&self, catalog: &Catalog) -> SearchResp {
        let query = petcalc::Query {
            lvl: self.lvl,
            floor: self.floor,
            random: self.random.unwrap_or(DEFAULT_RANDOM),
        };
        let available = query.available_points();

        let mut hits: Vec<SearchHit> = catalog
            .pets
            .iter()
            .filter(|p| self.passes_filters(p))
            .filter_map(|p| {
                let v = query.evaluate(GrowRange::from_array(p.grow, p.bprate));
                v.hit().then(|| SearchHit {
                    pet: PetDto::from(p),
                    stat: v.base.into(),
                    // 進位：0.4 點補不完一項，得花掉一整點
                    needed: v.needed.ceil() as i32,
                    spare: (available - v.needed).floor() as i32,
                })
            })
            .collect();

        // 餘裕多的排前面；同分用名字穩定排序，免得每次查順序都不一樣
        hits.sort_by(|a, b| {
            b.spare
                .cmp(&a.spare)
                .then_with(|| a.pet.name.cmp(&b.pet.name))
        });

        let total = hits.len();
        let truncated = self.limit > 0 && total > self.limit;
        if truncated {
            hits.truncate(self.limit);
        }
        SearchResp {
            available: available as i32,
            total,
            truncated,
            hits,
        }
    }

    /// 技能格數與屬性 —— 這兩項不用算，直接看圖鑑欄位。
    fn passes_filters(&self, pet: &Pet) -> bool {
        // 技能：原程式是 `pet.skills >= 輸入`。圖鑑沒填技能的（內建表就沒有）
        // 在有設下限時一律不算命中 —— 不知道就不能說它達標。
        if self.skills > 0 && pet.skills.unwrap_or(0) < self.skills {
            return false;
        }
        // 屬性：0 ＝ 不限；否則該屬必須 > 0。沒有屬性資料的同理不算命中。
        if self.element > 0 {
            match pet.element.and_then(|e| e.get(self.element - 1).copied()) {
                Some(v) if v > 0 => {}
                _ => return false,
            }
        }
        true
    }
}

// ── 啟動用的常數包 ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Constants {
    /// 完整的 111 筆成長係數表（cg-pet-calc 只有 53 筆且有兩筆是錯的）。
    pub full_rates: &'static [f64],
    pub max_tier: usize,
    /// 掉檔上限的**預設值**（原程式「檔次範圍」那欄的預設）。
    pub max_lost_per_axis: i32,
    /// 掉檔上限可以調到多大 —— 移植版的護欄，見 [`petcalc::MAX_LOSS_BOUND`]。
    pub max_loss_bound: i32,
    pub random_pool: i32,
    pub default_bprate: f64,
    pub default_random: [i32; AXES],
    /// 容差公式 `clamp((hi-lo)/200, tol_min, tol_max)` 的上下界。
    pub tol_min: f64,
    pub tol_max: f64,
    /// 百分比的顯示門檻（百分點）。原程式判定「這一格等於 100%」用的就是它，
    /// 前端要標「已確定」時**必須用同一個數**，不要自己編一個 99.9。
    pub percent_epsilon: f64,
    /// BP 軸的顯示名稱，順序即 `[HP, ATK, DEF, AGI, MP]`。
    pub axis_names: [&'static str; AXES],
    /// 能力的顯示名稱，順序即 [`StatDto`] 的欄位順序。
    pub stat_names: [&'static str; 7],
    /// 種族碼 → 名稱，**索引即碼**。名字以線上圖鑑（originmood）那份為準。
    ///
    /// 最後一筆是 [`unknown_race`](Self::unknown_race) 的容器，
    /// 編輯畫面的下拉要切掉它 —— 「其他」是給沒有種族資訊的舊資料落腳用的，
    /// 不該讓使用者主動挑。
    pub race_names: &'static [&'static str],
    /// 「種族不明」的碼。它一定是 `race_names` 的最後一筆。
    pub unknown_race: u8,
    /// 運算模式的按鈕列，順序即原程式主視窗的「智 野 無 體 力 防 敏 魔」。
    pub calc_modes: [CalcModeInfo; 8],
}

/// 一個運算模式按鈕：送回後端的鍵、按鈕上的字、以及要不要開側欄的加點方式。
#[derive(Debug, Clone, Copy, Serialize)]
pub struct CalcModeInfo {
    pub key: CalcMode,
    pub label: &'static str,
    /// 這個模式下側欄的「加點方式」才有意義。
    pub takes_manual_plan: bool,
}

impl CalcModeInfo {
    const fn new(key: CalcMode, label: &'static str, takes_manual_plan: bool) -> Self {
        Self {
            key,
            label,
            takes_manual_plan,
        }
    }
}

impl Constants {
    pub fn get() -> Self {
        Self {
            full_rates: &petcalc::FULL_RATES,
            max_tier: petcalc::MAX_TIER,
            max_lost_per_axis: MAX_LOST_PER_AXIS,
            max_loss_bound: MAX_LOSS_BOUND,
            random_pool: RANDOM_POOL,
            default_bprate: DEFAULT_BPRATE,
            default_random: DEFAULT_RANDOM,
            tol_min: petcalc::TOL_MIN,
            tol_max: petcalc::TOL_MAX,
            percent_epsilon: petcalc::stats::PERCENT_EPSILON,
            axis_names: ["體力", "力量", "強度", "速度", "魔法"],
            stat_names: ["生命", "魔力", "攻擊", "防禦", "敏捷", "精神", "回復"],
            race_names: &petdata::RACE_NAMES,
            unknown_race: petdata::RACE_UNKNOWN,
            calc_modes: [
                CalcModeInfo::new(CalcMode::Smart, "智", true),
                CalcModeInfo::new(CalcMode::Wild, "野", true),
                CalcModeInfo::new(CalcMode::None, "無", false),
                CalcModeInfo::new(CalcMode::Hp, "體", false),
                CalcModeInfo::new(CalcMode::Atk, "力", false),
                CalcModeInfo::new(CalcMode::Def, "防", false),
                CalcModeInfo::new(CalcMode::Agi, "敏", false),
                CalcModeInfo::new(CalcMode::Mp, "魔", false),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_mode_puts_everything_on_one_axis() {
        assert_eq!(PointMode::Atk.allocate(99, [1; AXES]), [0, 99, 0, 0, 0]);
        assert_eq!(PointMode::None.allocate(99, [1; AXES]), [0; AXES]);
    }

    #[test]
    fn ratio_allocation_is_exact() {
        // 10 點按 1:1:1:1:1 -> 每軸 2
        assert_eq!(PointMode::Ratio.allocate(10, [1; AXES]), [2; AXES]);
        // 除不盡時餘數要補回去，總和必須守恆
        for total in 0..200 {
            let got = PointMode::Ratio.allocate(total, [3, 1, 4, 1, 5]);
            assert_eq!(got.iter().sum::<i32>(), total.max(0), "total={total}");
        }
    }

    #[test]
    fn ratio_ignores_zero_and_negative_weights() {
        let got = PointMode::Ratio.allocate(10, [0, 5, 0, -3, 5]);
        assert_eq!(got, [0, 5, 0, 0, 5]);
    }

    #[test]
    fn forward_row_matches_hand_computed_stats() {
        // 檔次全 0、1 級、隨機檔全 0、無加點 -> 純基礎值
        let req = ForwardReq {
            grow: GrowDto {
                hp: 0,
                atk: 0,
                def: 0,
                agi: 0,
                mp: 0,
                bprate: DEFAULT_BPRATE,
            },
            lvl: 1,
            random: Some([0; AXES]),
            mode: PointMode::None,
            ratio: None,
            manual: None,
            not_order_point: 0,
            burst: None,
        };
        let row = forward_row(&req, 1);
        assert_eq!(row.stat.hp, 20);
        assert_eq!(row.stat.mp, 20);
        assert_eq!(row.stat.wis, 100);
        assert_eq!(row.bp_sum, 0.0);
    }

    #[test]
    fn manual_override_wins_over_mode() {
        let req = ForwardReq {
            grow: GrowDto {
                hp: 10,
                atk: 10,
                def: 10,
                agi: 10,
                mp: 10,
                bprate: DEFAULT_BPRATE,
            },
            lvl: 50,
            random: None,
            mode: PointMode::Hp,
            ratio: None,
            manual: Some([1, 2, 3, 4, 5]),
            not_order_point: 0,
            burst: None,
        };
        assert_eq!(req.manual_at(50), [1, 2, 3, 4, 5]);
    }

    #[test]
    fn not_order_point_reduces_the_pool() {
        let req = ForwardReq {
            grow: GrowDto {
                hp: 10,
                atk: 10,
                def: 10,
                agi: 10,
                mp: 10,
                bprate: DEFAULT_BPRATE,
            },
            lvl: 50,
            random: None,
            mode: PointMode::Hp,
            ratio: None,
            manual: None,
            not_order_point: 9, // 入手等級 10：1->10 的點不是玩家配的
            burst: None,
        };
        assert_eq!(req.manual_at(50), [40, 0, 0, 0, 0]);
    }

    /// 暴點：當前等級為止照 `mode`，之後的點改配到暴點那一軸。
    #[test]
    fn burst_switches_the_axis_after_the_current_level() {
        let req = ForwardReq {
            grow: GrowDto {
                hp: 10,
                atk: 10,
                def: 10,
                agi: 10,
                mp: 10,
                bprate: DEFAULT_BPRATE,
            },
            lvl: 30,
            random: None,
            mode: PointMode::Atk,
            ratio: None,
            manual: None,
            not_order_point: 0,
            burst: Some(PointMode::Hp),
        };
        // 30 級（＝當前等級）為止的 29 點還是全在力量
        assert_eq!(req.manual_at(30), [0, 29, 0, 0, 0]);
        // 之後每一級都改進體力
        assert_eq!(req.manual_at(31), [1, 29, 0, 0, 0]);
        assert_eq!(req.manual_at(50), [20, 29, 0, 0, 0]);
        // 往回看不該倒扣 —— 低於當前等級時就是單純的 mode 分配
        assert_eq!(req.manual_at(10), [0, 9, 0, 0, 0]);
        assert_eq!(req.manual_at(1), [0; AXES]);
    }

    /// 「下次」（`burst: None`）必須跟完全沒有暴點這回事一模一樣。
    #[test]
    fn burst_none_changes_nothing() {
        let mut req = ForwardReq {
            grow: GrowDto {
                hp: 10,
                atk: 10,
                def: 10,
                agi: 10,
                mp: 10,
                bprate: DEFAULT_BPRATE,
            },
            lvl: 30,
            random: None,
            mode: PointMode::Def,
            ratio: None,
            manual: None,
            not_order_point: 4,
            burst: None,
        };
        let plain: Vec<_> = (1..=60).map(|l| req.manual_at(l)).collect();
        // 暴點指到跟 mode 同一軸，效果也應該等於沒設
        req.burst = Some(PointMode::Def);
        let same: Vec<_> = (1..=60).map(|l| req.manual_at(l)).collect();
        assert_eq!(plain, same);
    }

    /// 加點總數守恆：暴點只換軸，不會憑空多出或吃掉點數。
    #[test]
    fn burst_conserves_the_point_total() {
        let req = ForwardReq {
            grow: GrowDto {
                hp: 10,
                atk: 10,
                def: 10,
                agi: 10,
                mp: 10,
                bprate: DEFAULT_BPRATE,
            },
            lvl: 25,
            random: None,
            mode: PointMode::Mp,
            ratio: None,
            manual: None,
            not_order_point: 9,
            burst: Some(PointMode::Agi),
        };
        for lvl in 1..=100 {
            let want = (lvl - 1 - 9).max(0);
            assert_eq!(req.manual_at(lvl).iter().sum::<i32>(), want, "lvl={lvl}");
        }
    }

    #[test]
    fn grow_dto_round_trips_through_the_engine_type() {
        let d = GrowDto {
            hp: 1,
            atk: 2,
            def: 3,
            agi: 4,
            mp: 5,
            bprate: 0.2,
        };
        let back: GrowDto = GrowRange::from(d).into();
        assert_eq!(
            (back.hp, back.atk, back.def, back.agi, back.mp),
            (1, 2, 3, 4, 5)
        );
    }

    #[test]
    fn constants_expose_the_full_rate_table_not_the_truncated_one() {
        let c = Constants::get();
        assert_eq!(c.full_rates.len(), 111);
        // cg-pet-calc 的表在這兩格是錯的（2.04 / 2.08）
        assert_eq!(c.full_rates[51], 2.14);
        assert_eq!(c.full_rates[52], 2.18);
    }

    /// 前端的種族下拉是 `race_names.slice(0, unknown_race)` ——
    /// 這條成立的前提是「索引即碼」而且「不明」剛好排在最後一筆。
    #[test]
    fn the_race_list_is_indexed_by_code_with_unknown_last() {
        let c = Constants::get();
        assert_eq!(c.unknown_race as usize, c.race_names.len() - 1);
        for (code, name) in c.race_names.iter().enumerate() {
            assert_eq!(race_name(code as u8), *name, "碼 {code} 的名字對不上");
        }
        // 切掉最後一筆之後剩下的才是真正的遊戲種族
        assert_eq!(&c.race_names[..c.unknown_race as usize].len(), &9);
    }

    fn guess_req(calc_mode: CalcMode, manual: Option<ManualPlanDto>) -> GuessReq {
        GuessReq {
            grow: GrowDto {
                hp: 27,
                atk: 16,
                def: 25,
                agi: 15,
                mp: 37,
                bprate: DEFAULT_BPRATE,
            },
            target: TargetDto {
                lvl: 20,
                hp: 0,
                mp: 0,
                atk: 0,
                def: 0,
                agi: 0,
            },
            not_order_point: 0,
            calc_mode,
            manual,
            catch_stat: None,
            mode: MatchModeDto::Auto,
            exhaustive: false,
            tolerance: None,
            target_grow: None,
            limit: 0,
            ranges: RangeLimitsDto::default(),
        }
    }

    #[test]
    fn single_axis_modes_map_to_their_own_axis() {
        let cases = [
            (CalcMode::Hp, [40, 0, 0, 0, 0]),
            (CalcMode::Atk, [0, 40, 0, 0, 0]),
            (CalcMode::Def, [0, 0, 40, 0, 0]),
            (CalcMode::Agi, [0, 0, 0, 40, 0]),
            (CalcMode::Mp, [0, 0, 0, 0, 40]),
        ];
        for (mode, allowed) in cases {
            let b = guess_req(mode, None).manual_bounds();
            assert!(b.allows(allowed), "{mode:?} 不接受 {allowed:?}");
            assert!(!b.allows([8, 8, 8, 8, 8]), "{mode:?} 竟然接受散開的加點");
        }
    }

    /// 「無」不是「加了 0 點」，而是那 `lvl - 1` 點根本沒配掉。
    #[test]
    fn none_mode_marks_every_level_point_as_unallocated() {
        let req = guess_req(CalcMode::None, None);
        assert_eq!(req.effective_not_order_point(), 19);
        assert!(req.manual_bounds().allows([0; AXES]));
        assert!(!req.manual_bounds().allows([1, 0, 0, 0, 0]));
        // 其他模式不該被動到
        assert_eq!(
            guess_req(CalcMode::Smart, None).effective_not_order_point(),
            0
        );
    }

    #[test]
    fn sidebar_plan_only_applies_to_smart_and_wild() {
        let plan = ManualPlanDto::Fixed {
            points: [1, 2, 3, 4, 5],
        };
        for mode in [CalcMode::Smart, CalcMode::Wild] {
            let b = guess_req(mode, Some(plan)).manual_bounds();
            assert!(b.allows([1, 2, 3, 4, 5]), "{mode:?} 沒吃到側欄的指定加點");
            assert!(!b.allows([5, 4, 3, 2, 1]));
        }
        // 「體」自己就把加點釘死了，側欄設什麼都不該蓋掉它
        let b = guess_req(CalcMode::Hp, Some(plan)).manual_bounds();
        assert!(!b.allows([1, 2, 3, 4, 5]));
        assert!(b.allows([15, 0, 0, 0, 0]));
    }

    #[test]
    fn manual_plans_convert_to_the_matching_bounds() {
        let free: ManualBounds = ManualPlanDto::Free.into();
        assert!(free.allows([9, 9, 9, 9, 9]));

        let mixed: ManualBounds = ManualPlanDto::Mixed {
            axes: [true, false, true, false, false],
        }
        .into();
        assert!(mixed.allows([7, 0, 3, 0, 0]));
        assert!(!mixed.allows([7, 1, 3, 0, 0]));

        let ranged: ManualBounds = ManualPlanDto::Range {
            bounds: [[1, 3], [0, 0], [2, 4], [0, 9], [0, 0]],
        }
        .into();
        assert!(ranged.allows([2, 0, 3, 5, 0]));
        assert!(!ranged.allows([0, 0, 3, 5, 0]), "低於下界卻被接受");
        assert!(!ranged.allows([4, 0, 3, 5, 0]), "高於上界卻被接受");
        assert_eq!(ranged.min_total(), 3);
    }

    /// 入手等級填錯要當場報錯，不能讓它安靜地把解濾光。
    #[test]
    fn catch_level_must_not_exceed_the_current_level() {
        let stat = |lvl| TargetDto {
            lvl,
            hp: 100,
            mp: 100,
            atk: 30,
            def: 30,
            agi: 30,
        };
        let with = |lvl| GuessReq {
            catch_stat: Some(stat(lvl)),
            ..guess_req(CalcMode::Wild, None)
        };

        // guess_req 的當前等級是 20
        assert!(with(1).check_catch().is_ok());
        assert!(with(20).check_catch().is_ok(), "入手等級＝當前等級是合法的");
        assert!(with(0).check_catch().is_err());

        let err = with(21).check_catch().unwrap_err();
        assert!(
            err.contains("21") && err.contains("20"),
            "錯誤訊息沒說清楚：{err}"
        );

        // 沒填就沒事
        assert!(guess_req(CalcMode::Wild, None).check_catch().is_ok());
    }

    /// 入手能力要真的傳進引擎，不是收下來就丟掉。
    #[test]
    fn catch_stat_reaches_the_engine_options() {
        let stat = TargetDto {
            lvl: 12,
            hp: 200,
            mp: 150,
            atk: 40,
            def: 35,
            agi: 30,
        };
        let req = GuessReq {
            catch_stat: Some(stat),
            ..guess_req(CalcMode::Wild, None)
        };
        let opts = GuessOptions::from(&req);
        let catch = opts.catch.expect("入手能力沒有傳進 GuessOptions");
        assert_eq!(catch.lvl, 12);
        assert_eq!(catch.to_array(), [200, 150, 40, 35, 30]);

        assert!(GuessOptions::from(&guess_req(CalcMode::Wild, None))
            .catch
            .is_none());
    }

    // ── 線上格式 ────────────────────────────────────────────────────────────
    //
    // 這兩條用**字面 JSON**釘住前端實際送出來的形狀。欄位名打錯的話 serde 會
    // 安靜地套用 `#[serde(default)]`，條件就默默失效了 —— 沒有任何錯誤訊息，
    // 使用者只會覺得「勾了好像沒差」。所以寧可在這裡撞牆。

    /// 對應 `app.js` 的 `buildGuessReq()`。
    #[test]
    fn the_frontend_guess_payload_deserializes() {
        let json = r#"{
            "grow": {"hp":27,"atk":16,"def":25,"agi":15,"mp":37,"bprate":0.2},
            "target": {"lvl":40,"hp":500,"mp":300,"atk":80,"def":70,"agi":60},
            "not_order_point": 19,
            "calc_mode": "wild",
            "manual": {"kind":"free"},
            "catch_stat": {"lvl":20,"hp":250,"mp":160,"atk":40,"def":35,"agi":30},
            "mode": "auto",
            "exhaustive": true,
            "tolerance": null,
            "target_grow": null,
            "limit": 0
        }"#;
        let req: GuessReq = serde_json::from_str(json).expect("前端的 guess 請求解不開");
        assert_eq!(req.calc_mode, CalcMode::Wild);
        assert_eq!(req.catch_stat.expect("catch_stat 沒被認出來").lvl, 20);
        assert!(req.check_catch().is_ok());
        let catch = GuessOptions::from(&req).catch.expect("入手能力沒進到引擎");
        assert_eq!(catch.to_array(), [250, 160, 40, 35, 30]);

        // 沒勾「使用入手能力」時前端送 null
        let off = json.replace(
            r#""catch_stat": {"lvl":20,"hp":250,"mp":160,"atk":40,"def":35,"agi":30}"#,
            r#""catch_stat": null"#,
        );
        let req: GuessReq = serde_json::from_str(&off).unwrap();
        assert!(GuessOptions::from(&req).catch.is_none());
    }

    /// 對應 `app.js` 的 `forwardBase()`。
    #[test]
    fn the_frontend_forward_payload_deserializes() {
        let json = r#"{
            "grow": {"hp":27,"atk":16,"def":25,"agi":15,"mp":37,"bprate":0.2},
            "lvl": 30,
            "random": [2,2,2,2,2],
            "mode": "atk",
            "manual": null,
            "not_order_point": 0,
            "burst": "hp"
        }"#;
        let req: ForwardReq = serde_json::from_str(json).expect("前端的 forward 請求解不開");
        assert_eq!(req.mode, PointMode::Atk);
        assert_eq!(req.burst, Some(PointMode::Hp));
        assert_eq!(
            req.manual_at(31),
            [1, 29, 0, 0, 0],
            "暴點沒有從下一級起換軸"
        );

        // 「下次」送 null ＝ 整段都照 mode 走
        let next: ForwardReq =
            serde_json::from_str(&json.replace(r#""burst": "hp""#, r#""burst": null"#)).unwrap();
        assert_eq!(next.burst, None);
        assert_eq!(next.manual_at(31), [0, 30, 0, 0, 0]);
    }

    #[test]
    fn every_calc_mode_button_is_wired_to_a_distinct_mode() {
        let modes = Constants::get().calc_modes;
        assert_eq!(modes.len(), 8);
        assert_eq!(modes.map(|m| m.label).join(""), "智野無體力防敏魔");
        for (i, m) in modes.iter().enumerate() {
            assert!(
                !modes[..i].iter().any(|p| p.key == m.key),
                "{} 重複了",
                m.label
            );
            assert_eq!(m.takes_manual_plan, m.key.takes_manual_plan());
        }
    }
}
