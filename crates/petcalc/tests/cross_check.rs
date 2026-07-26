//! 與 `cg-pet-calc` 的差分測試。
//!
//! 向量由 `tools/gen/gen_vectors.mjs` 產生（`node tools/gen/gen_vectors.mjs`），
//! 已簽入 `crates/petcalc/tests/vectors/`，所以這個測試不需要 Node 也能跑。
//!
//! 涵蓋範圍：
//!
//! * **檔次 0..=110** — 全範圍逐位元一致。
//! * **精神／回復** — 不對照。cg-pet-calc 的 `Stat` 建構子只收 5 個能力，
//!   `calcRealNum()` 算出來的 wis/res 被靜默丟棄，沒有 baseline 可比。
//!
//! 歷史：cg-pet-calc 的 `fullRates` 原本只到 index 52，且 51/52 是錯的
//! （2.04 / 2.08），所以這裡一度只能對照到檔次 50，並且有一個
//! 「刻意分歧」的測試釘住差異。該表已依噬生・魔物觀測者 v3.12 的完整表修正成
//! 111 筆（`cg-pet-calc/lib/Utils.mjs`，回歸測試在 `test/fullRates.js`），
//! 分歧測試因此換成 [`high_tiers_agree_with_cg_pet_calc`]。

use petcalc::{Bp, GrowRange, FULL_RATES, MAX_TIER};

const FORWARD_VECTORS: &str = include_str!("vectors/forward.txt");

struct Vector {
    tiers: [i32; 5],
    bprate: f64,
    lvl: i32,
    /// 顯示順序：血 魔 攻 防 敏
    stats: [i64; 5],
    sum_base_bp: f64,
    sum_full_bp: f64,
}

fn parse(line: &str) -> Vector {
    let f: Vec<&str> = line.split('|').collect();
    assert_eq!(f.len(), 6, "格式錯誤: {line}");

    let nums = |s: &str| -> Vec<f64> { s.split(',').map(|n| n.parse().unwrap()).collect() };

    let t = nums(f[0]);
    let s = nums(f[3]);
    Vector {
        tiers: [
            t[0] as i32,
            t[1] as i32,
            t[2] as i32,
            t[3] as i32,
            t[4] as i32,
        ],
        bprate: f[1].parse().unwrap(),
        lvl: f[2].parse().unwrap(),
        stats: [
            s[0] as i64,
            s[1] as i64,
            s[2] as i64,
            s[3] as i64,
            s[4] as i64,
        ],
        sum_base_bp: f[4].parse().unwrap(),
        sum_full_bp: f[5].parse().unwrap(),
    }
}

#[test]
fn forward_calc_matches_cg_pet_calc_across_all_tiers() {
    let mut checked = 0;

    for (i, line) in FORWARD_VECTORS.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v = parse(line);
        let g = GrowRange::from_array(v.tiers, v.bprate);
        let at = g.calc_bp_at_level(v.lvl, None);
        let got = at.base_bp.calc_real_num();

        let expected = v.stats;
        let actual = [got.hp, got.mp, got.atk, got.def, got.agi];
        assert_eq!(
            actual,
            expected,
            "第 {} 行不一致\n  tiers={:?} bprate={} lvl={}\n  rust={:?}\n  js  ={:?}",
            i + 1,
            v.tiers,
            v.bprate,
            v.lvl,
            actual,
            expected
        );

        assert!(
            (at.sum_base_bp - v.sum_base_bp).abs() < 1e-9,
            "第 {} 行 sumBaseBP 不一致: rust={} js={}",
            i + 1,
            at.sum_base_bp,
            v.sum_base_bp
        );
        assert!(
            (at.sum_full_bp - v.sum_full_bp).abs() < 1e-9,
            "第 {} 行 sumFullBP 不一致: rust={} js={}",
            i + 1,
            at.sum_full_bp,
            v.sum_full_bp
        );

        checked += 1;
    }

    assert!(
        checked >= 1200,
        "向量太少（{checked}），vectors/ 可能沒重新產生"
    );
}

/// 曾經壞掉的高檔次區段（51..=110）現在兩邊必須一致。
///
/// 這個測試取代了舊的 `tiers_51_and_52_intentionally_diverge_from_cg_pet_calc`。
/// 向量檔裡 51..=110 每一檔都有 4 個等級的樣本，這裡再額外驗一次
/// 「我們的表值本身」是對的 —— 兩邊同時抄錯同一張表的話，
/// 單純的差分比對是抓不到的。
#[test]
fn high_tiers_agree_with_cg_pet_calc() {
    let mut checked = 0;
    for line in FORWARD_VECTORS.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v = parse(line);
        if v.tiers.iter().all(|&t| t <= 50) {
            continue;
        }

        let g = GrowRange::from_array(v.tiers, v.bprate);
        let got = g.calc_bp_at_level(v.lvl, None).base_bp.calc_real_num();
        assert_eq!(
            [got.hp, got.mp, got.atk, got.def, got.agi],
            v.stats,
            "高檔次不一致: tiers={:?} lvl={}",
            v.tiers,
            v.lvl
        );
        checked += 1;
    }
    assert!(checked >= 300, "高檔次向量太少（{checked}）");

    // 直接釘死曾經寫錯的兩筆，以及表尾
    assert_eq!(FULL_RATES[51], 2.14, "51 又被改回 cg-pet-calc 的舊錯值了？");
    assert_eq!(FULL_RATES[52], 2.18, "52 又被改回 cg-pet-calc 的舊錯值了？");
    assert_eq!(FULL_RATES[MAX_TIER], 4.615);
    assert_eq!(FULL_RATES.len(), 111);

    // 單調遞增 —— 舊表在 50→51 這裡是往回掉的
    for n in 1..=MAX_TIER {
        assert!(
            FULL_RATES[n] > FULL_RATES[n - 1],
            "檔次 {n} 沒有比 {} 大",
            n - 1
        );
    }
}

/// 精神／回復沒有 JS baseline，改用矩陣定義直接驗。
#[test]
fn wis_and_res_follow_the_documented_matrix() {
    // 單位 BP 打在體力軸：精神 -0.3、回復 +0.8
    let s = Bp::new(10.0, 0.0, 0.0, 0.0, 0.0).calc_real_num();
    assert_eq!(s.wis, 100 + (-3));
    assert_eq!(s.res, 100 + 8);

    // 打在魔法軸：精神 +0.8、回復 -0.3
    let s = Bp::new(0.0, 0.0, 0.0, 0.0, 10.0).calc_real_num();
    assert_eq!(s.wis, 100 + 8);
    assert_eq!(s.res, 100 + (-3));
}
