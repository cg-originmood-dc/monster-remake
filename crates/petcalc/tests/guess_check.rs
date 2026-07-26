//! 推算路徑與 `cg-pet-calc` 的差分測試。
//!
//! `cross_check.rs` 只比正向計算；這裡比的是反推 —— 由觀測能力推回
//! `(檔次, 加點, 隨機檔)`。這是移植中最容易出現細微行為差異的一段，
//! 光靠正向對拍是抓不到的。
//!
//! 向量由 `tools/gen/gen_guess_vectors.mjs` 產生並簽入 `crates/petcalc/tests/vectors/guess.txt`。
//!
//! # 比什麼、不比什麼
//!
//! * **精確案例** — 解集合必須**完全相同**。Rust 端用
//!   `MatchMode::Exact` + `RandomSearch::Ball { radius: 1 }`，
//!   這才是 cg-pet-calc 的同一個演算法。
//! * **近似案例** — cg-pet-calc 整數列舉無解、退回連續 LP 可行性求解
//!   （`isApproximate = true`）。我們沒有移植那條回退路徑，改用
//!   原程式的容差模型，所以**解不會一樣**，只驗「同樣推得出東西」。

use petcalc::{solve, GrowRange, GuessOptions, MatchMode, RandomSearch, Target, AXES};
use std::collections::BTreeSet;

const GUESS_VECTORS: &str = include_str!("vectors/guess.txt");

struct Case {
    label: String,
    tiers: [i32; AXES],
    bprate: f64,
    lvl: i32,
    /// 顯示順序：血 魔 攻 防 敏
    stats: [i64; AXES],
    not_order_point: i32,
    /// `"檔次|加點|隨機檔"`，排序後便於集合比對
    solutions: BTreeSet<String>,
    approximate: bool,
}

fn five(s: &str) -> [i64; AXES] {
    let v: Vec<i64> = s.split(',').map(|n| n.parse().unwrap()).collect();
    assert_eq!(v.len(), AXES, "欄位數不是 5: {s}");
    [v[0], v[1], v[2], v[3], v[4]]
}

fn parse_cases() -> Vec<Case> {
    let mut cases: Vec<Case> = Vec::new();
    for line in GUESS_VECTORS.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split('|').collect();
        match f[0] {
            "CASE" => {
                let t = five(f[2]);
                cases.push(Case {
                    label: f[1].to_string(),
                    tiers: [
                        t[0] as i32,
                        t[1] as i32,
                        t[2] as i32,
                        t[3] as i32,
                        t[4] as i32,
                    ],
                    bprate: f[3].parse().unwrap(),
                    lvl: f[4].parse().unwrap(),
                    stats: five(f[5]),
                    not_order_point: f[6].parse().unwrap(),
                    solutions: BTreeSet::new(),
                    approximate: false,
                });
            }
            "SOL" => {
                let c = cases.last_mut().expect("SOL 出現在 CASE 之前");
                if f[4] == "1" {
                    c.approximate = true;
                }
                c.solutions.insert(format!("{}|{}|{}", f[1], f[2], f[3]));
            }
            other => panic!("未知的列種類: {other}"),
        }
    }
    cases
}

impl Case {
    fn catalog(&self) -> GrowRange {
        GrowRange::from_array(self.tiers, self.bprate)
    }

    fn target(&self) -> Target {
        let s = self.stats;
        Target::new(self.lvl, s[0], s[1], s[2], s[3], s[4])
    }

    fn opts(&self, mode: MatchMode) -> GuessOptions {
        GuessOptions {
            not_order_point: self.not_order_point,
            mode,
            random: RandomSearch::Ball { radius: 1 },
            ..Default::default()
        }
    }
}

fn key(c: &petcalc::Candidate) -> String {
    let g = c.grow.to_array();
    let fmt = |a: [i32; AXES]| a.map(|n| n.to_string()).join(",");
    format!("{}|{}|{}", fmt(g), fmt(c.manual), fmt(c.random))
}

/// 精確案例：解集合必須逐項相同。
#[test]
fn exact_cases_match_cg_pet_calc_solution_for_solution() {
    let cases = parse_cases();
    assert!(
        cases.len() >= 10,
        "案例太少（{}），向量可能沒重新產生",
        cases.len()
    );

    let mut compared = 0;
    for c in cases.iter().filter(|c| !c.approximate) {
        let got = solve(c.catalog(), c.target(), c.opts(MatchMode::Exact));
        let ours: BTreeSet<String> = got.candidates.iter().map(key).collect();

        let missing: Vec<&String> = c.solutions.difference(&ours).collect();
        let extra: Vec<&String> = ours.difference(&c.solutions).collect();

        assert!(
            missing.is_empty() && extra.is_empty(),
            "{} 解集合不一致\n  js={} rust={}\n  js 有我們沒有（前 5）: {:?}\n  我們有 js 沒有（前 5）: {:?}",
            c.label,
            c.solutions.len(),
            ours.len(),
            missing.iter().take(5).collect::<Vec<_>>(),
            extra.iter().take(5).collect::<Vec<_>>(),
        );
        compared += 1;
    }
    assert!(compared >= 9, "精確案例只比到 {compared} 個");
}

/// 每一組推出來的解，正算回去都要等於觀測能力。
///
/// 這一條不依賴 cg-pet-calc —— 就算兩邊一起錯，這裡也會抓到。
#[test]
fn every_solution_reproduces_the_observed_stats() {
    for c in parse_cases().iter().filter(|c| !c.approximate) {
        let got = solve(c.catalog(), c.target(), c.opts(MatchMode::Exact));
        assert!(!got.candidates.is_empty(), "{} 無解", c.label);
        for cand in &got.candidates {
            let s = cand.stat;
            assert_eq!(
                [s.hp, s.mp, s.atk, s.def, s.agi],
                c.stats,
                "{} 的候選解正算回去對不上：{:?}",
                c.label,
                cand
            );
            assert_eq!(
                cand.random.iter().sum::<i32>(),
                10,
                "{} 隨機檔總和不是 10",
                c.label
            );
        }
    }
}

/// 近似案例：cg-pet-calc 得退回連續 LP 才有解，我們用容差模型也要推得出來。
///
/// 這正是移植容差模型的理由 —— 不必為了「整數列舉無解」另外接一套 LP 求解器。
#[test]
fn approximate_cases_are_solved_by_the_tolerance_model() {
    let cases = parse_cases();
    let approx: Vec<&Case> = cases.iter().filter(|c| c.approximate).collect();
    assert!(!approx.is_empty(), "向量裡沒有近似案例，這個測試就沒意義了");

    for c in approx {
        // 先確認它真的是整數列舉推不出來的案例
        let exact = solve(c.catalog(), c.target(), c.opts(MatchMode::Exact));
        assert!(
            exact.candidates.is_empty(),
            "{} 被標成近似案例，但精確模式竟然有解 —— 向量該重產了",
            c.label
        );

        // 容差模式要接得住
        let tolerant = solve(c.catalog(), c.target(), c.opts(MatchMode::Tolerant));
        assert!(
            !tolerant.candidates.is_empty(),
            "{} 容差模式也推不出來 —— 這是移植容差模型的主要理由，不該失敗",
            c.label
        );
        assert!(tolerant.candidates.iter().all(|x| x.approximate));

        // Auto 要降級到「還推得出來的最緊模式」，不是無腦跳到最鬆的那個
        let observer = solve(c.catalog(), c.target(), c.opts(MatchMode::Observer));
        let auto = solve(c.catalog(), c.target(), c.opts(MatchMode::Auto));
        let (want_mode, want_len) = if observer.candidates.is_empty() {
            (MatchMode::Tolerant, tolerant.candidates.len())
        } else {
            (MatchMode::Observer, observer.candidates.len())
        };
        eprintln!(
            "{}：observer {} 組、tolerant {} 組 → Auto 停在 {want_mode:?}",
            c.label,
            observer.candidates.len(),
            tolerant.candidates.len(),
        );
        assert_eq!(auto.mode, want_mode, "{} 的 Auto 停錯模式", c.label);
        assert_eq!(auto.candidates.len(), want_len);
    }
}

/// 釘死「為什麼容差預設值是 0.1 而不是 0.01」。
///
/// 一開始我假設容差公式裡的 `hi-lo` 就是整數能力格的寬度 1
/// （→ `1/200 = 0.005` → 被下限接住 → `TOL_MIN` = 0.01）。
/// 實測推翻了這個假設：對「純白液態史萊姆 59 級」窮舉所有整數組合後，
/// 最小可達的最大單項誤差是 **0.07**，落在 [0.01, 0.1] 的中間偏上。
///
/// 這個測試同時驗證兩件事：
/// 1. `TOL_MIN` 確實不夠用（所以預設值不能取下限）
/// 2. `TOL_MAX` 夠用（所以不需要超出原程式的上界）
#[test]
fn tolerance_default_needs_the_upper_bound_not_the_lower() {
    let cases = parse_cases();
    let c = cases
        .iter()
        .find(|c| c.approximate)
        .expect("需要至少一個近似案例");

    let with = |tol: f64| {
        solve(
            c.catalog(),
            c.target(),
            GuessOptions {
                tolerance: Some(tol),
                ..c.opts(MatchMode::Tolerant)
            },
        )
        .candidates
        .len()
    };

    assert_eq!(
        with(petcalc::TOL_MIN),
        0,
        "{}: 0.01 就夠的話，預設值應該改回下限",
        c.label
    );
    assert_eq!(with(0.05), 0, "{}: 實測殘差 0.07，0.05 不該有解", c.label);
    assert!(with(petcalc::TOL_MAX) > 0, "{}: 0.1 應該要有解", c.label);

    // 預設值（不指定 tolerance）必須等同於用上界
    let default = solve(c.catalog(), c.target(), c.opts(MatchMode::Tolerant));
    assert_eq!(default.candidates.len(), with(petcalc::TOL_MAX));
}

/// `Observer` 只在檔次落在成長係數表不規則處才放寬 —— 精確模式推不出來的案例，
/// 這條**更緊**的規則也要接得住。
///
/// 這是「原程式為什麼推得動野寵」的直接證據：不需要全軸開 ±0.1，
/// 只要修好 `檔次 % 5 ∈ {0, 1}` 那幾軸的取整邊界就夠了。
#[test]
fn the_observer_rule_alone_solves_what_exact_cannot() {
    let cases = parse_cases();
    let c = cases
        .iter()
        .find(|c| c.approximate)
        .expect("需要至少一個近似案例");

    assert!(solve(c.catalog(), c.target(), c.opts(MatchMode::Exact))
        .candidates
        .is_empty());

    let observer = solve(c.catalog(), c.target(), c.opts(MatchMode::Observer));
    assert!(
        !observer.candidates.is_empty(),
        "{}: Observer 規則推不出來 —— 那原程式當初是怎麼推的？",
        c.label
    );
    assert!(observer.candidates.iter().all(|x| x.approximate));
}

/// `Observer` 的解集合必須是 `Exact` 的**超集**。
///
/// 每軸的偏移量第一個恆為 0（原程式也是先試 `Trunc(v)` 才試 `Trunc(v ± tol)`），
/// 所以精確模式找得到的解，放寬之後一定還在。反過來不成立。
///
/// 這條同時擋住兩種退化：偏移量把原本的解擠掉（漏解），
/// 或是規律檔次上偷偷放寬了（不該多出來的解）。
#[test]
fn the_observer_rule_never_loses_an_exact_solution() {
    let mut widened = 0;
    for c in parse_cases() {
        let exact = solve(c.catalog(), c.target(), c.opts(MatchMode::Exact));
        let observer = solve(c.catalog(), c.target(), c.opts(MatchMode::Observer));

        for want in &exact.candidates {
            assert!(
                observer
                    .candidates
                    .iter()
                    .any(|got| got.grow == want.grow
                        && got.manual == want.manual
                        && got.random == want.random),
                "{}: Observer 漏掉了 Exact 找到的解 {:?}",
                c.label,
                want.grow.to_array()
            );
        }
        // 精確模式找到的那些，在 Observer 底下也不該被標成「靠偏移才成立」
        assert_eq!(
            observer.candidates.iter().filter(|x| !x.approximate).count(),
            exact.candidates.len(),
            "{}: 不需偏移的解數量對不上",
            c.label
        );
        if observer.candidates.len() > exact.candidates.len() {
            widened += 1;
        }
        // 印出來是為了跟 cg-pet-calc 的 `test/observer.js` 對數字 ——
        // 同一套規則移到 JS 上，每個案例的解數必須一樣。
        eprintln!(
            "{}：exact {} 組 → observer {} 組（其中 {} 組靠偏移）",
            c.label,
            exact.candidates.len(),
            observer.candidates.len(),
            observer.candidates.iter().filter(|x| x.approximate).count(),
        );
    }
    assert!(widened > 0, "沒有任何案例被放寬，Observer 等於沒作用");
}

/// 窮舉隨機檔不會比 ±1 鄰域少找到解。
///
/// 鄰域法靠反矩陣代數估計起點，估計偏掉就會漏解；窮舉 1001 組不會。
/// 兩者在這批案例上應該一致 —— 若哪天不一致，代表鄰域半徑不夠用。
#[test]
fn exhaustive_random_search_is_a_superset_of_the_ball() {
    for c in parse_cases().iter().filter(|c| !c.approximate) {
        let ball = solve(c.catalog(), c.target(), c.opts(MatchMode::Exact));
        let exhaustive = solve(
            c.catalog(),
            c.target(),
            GuessOptions {
                random: RandomSearch::Exhaustive,
                ..c.opts(MatchMode::Exact)
            },
        );

        let ball_keys: BTreeSet<String> = ball.candidates.iter().map(key).collect();
        let ex_keys: BTreeSet<String> = exhaustive.candidates.iter().map(key).collect();

        let lost: Vec<&String> = ball_keys.difference(&ex_keys).collect();
        assert!(
            lost.is_empty(),
            "{} 鄰域法找到但窮舉沒找到: {:?}",
            c.label,
            lost
        );
        assert!(
            ex_keys.len() >= ball_keys.len(),
            "{} 窮舉({}) 比鄰域({}) 少",
            c.label,
            ex_keys.len(),
            ball_keys.len()
        );
    }
}
