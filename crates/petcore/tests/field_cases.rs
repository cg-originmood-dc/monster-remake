//! 實測案例：Discord 的「/算檔」機器人（跑 cg-pet-calc）在群裡回過的實際輸出。
//!
//! 這些不是我們自己造的向量 —— 是真的有人拿真的寵物去查、貼在群裡的結果。
//! 六隻的圖鑑檔次都跟 `public/data` 那份逐字相同，所以兩邊算的是同一件事，
//! 解的**數量**與**掉檔範圍**可以直接對。
//!
//! ⚠️ **機器人印出來的筆數有兩種**，比對前要先分清楚：
//!
//! * 解 ≤ 100 組 → 「共有: N 個可能解」，N 就是真的筆數，可以直接對。
//! * 解 > 100 組 → 「共有 N 個結果，超過 100個組合，不顯示詳細結果」，
//!   這條 N **被扣掉了 10**（`Utils.mjs` 印的是 `results.length - limit`，
//!   而這條分支根本沒有列任何一筆出來，沒有東西該扣）。
//!
//! 所以底下三個「> 100」的案例，期望值是機器人印的數字 **＋10**
//! —— 那才是 cg-pet-calc 自己 `results.length` 的值，本機跑過確認過。
//!
//! 另外 cg-pet-calc 印的「總掉檔」是負數（`LostBP = grSum - sumBP`，符號反了），
//! 這裡照正數比。

use petcore::custom::NullStore;
use petcore::Engine;
use serde_json::{json, Value};

fn engine() -> Engine {
    Engine::new(Box::new(NullStore))
}

/// 機器人的指令是 `/算檔 名字 <等級> 血 魔 攻 防 敏`（等級 1 可省略）。
struct Case {
    name: &'static str,
    grow: [i32; 5],
    lvl: i32,
    /// 血 魔 攻 防 敏
    stat: [i32; 5],
}

fn run(c: &Case) -> Value {
    let [hp_t, atk_t, def_t, agi_t, mp_t] = c.grow;
    let [hp, mp, atk, def, agi] = c.stat;
    engine()
        .dispatch(
            "guess",
            json!({ "req": {
                "grow": {"hp":hp_t,"atk":atk_t,"def":def_t,"agi":agi_t,"mp":mp_t,"bprate":0.2},
                "target": {"lvl":c.lvl,"hp":hp,"mp":mp,"atk":atk,"def":def,"agi":agi},
                "not_order_point": 0, "calc_mode": "smart", "mode": "exact",
                "exhaustive": true, "limit": 0
            }}),
        )
        .unwrap_or_else(|e| panic!("{} 推算失敗：{e}", c.name))
}

/// 每軸的掉檔範圍，對應機器人印的「掉檔可能解範圍」。
///
/// ⚠️ **不能拿 `candidates` 算** —— 那個欄位有 200 筆的硬上限（`truncated: true`），
/// 只掃它會少掉尾巴上的解。`distribution.lost_marginal` 才是對**全部**候選
/// 加權統計出來的 5×5 表（列＝軸，行＝掉 0..4 檔），權重不為零就代表那個掉檔量有解。
fn loss_range(resp: &Value) -> [[i32; 2]; 5] {
    let m = resp["distribution"]["lost_marginal"]
        .as_array()
        .expect("沒有掉檔分布");
    std::array::from_fn(|axis| {
        let row = m[axis].as_array().unwrap();
        let hit: Vec<i32> = (0..row.len() as i32)
            .filter(|&i| row[i as usize].as_f64() != Some(0.0))
            .collect();
        [*hit.first().expect("整條都是 0"), *hit.last().unwrap()]
    })
}

/// 一級的野生狀態：沒有任何加點，解空間只由（掉檔 × 隨機檔）撐開。
///
/// 機器人回的是 44 個可能解、掉檔範圍 2~4 / 2~4 / 3~4 / 2~4 / 3~4。
#[test]
fn the_bot_s_level_one_duck_comes_out_the_same() {
    let c = Case {
        name: "小白鴨",
        grow: [40, 45, 10, 20, 10],
        lvl: 1,
        stat: [118, 70, 47, 29, 30],
    };
    let resp = run(&c);

    assert_eq!(resp["total"], json!(44), "解的筆數跟機器人不同");
    assert_eq!(
        loss_range(&resp),
        [[2, 4], [2, 4], [3, 4], [2, 4], [3, 4]],
        "掉檔可能解範圍跟機器人不同"
    );
}

/// 17 級、有加點的版本 —— 同一隻鴨子，機器人回 4 個解。
#[test]
fn the_bot_s_level_seventeen_duck_comes_out_the_same() {
    let c = Case {
        name: "小白鴨",
        grow: [40, 45, 10, 20, 10],
        lvl: 17,
        stat: [481, 291, 182, 71, 68],
    };
    let resp = run(&c);

    assert_eq!(resp["total"], json!(4), "解的筆數跟機器人不同");
    assert_eq!(
        loss_range(&resp),
        [[1, 1], [0, 1], [2, 3], [2, 3], [1, 1]],
        "掉檔可能解範圍跟機器人不同"
    );
}

/// 機器人回 52 個解。
#[test]
fn the_bot_s_baby_bomb_comes_out_the_same() {
    let c = Case {
        name: "寶寶炸彈",
        grow: [18, 40, 10, 48, 9],
        lvl: 17,
        stat: [400, 315, 170, 76, 116],
    };
    let resp = run(&c);
    assert_eq!(resp["total"], json!(52), "解的筆數跟機器人不同");
}

/// 機器人印 455（＝真值 465 被扣掉 10），掉檔範圍 0~4 四軸滿開、魔法軸 2~4。
#[test]
fn the_bot_s_deep_sea_bird_comes_out_the_same() {
    let c = Case {
        name: "深海鳥魔",
        grow: [30, 7, 13, 27, 48],
        lvl: 1,
        stat: [106, 138, 29, 34, 33],
    };
    let resp = run(&c);

    assert_eq!(
        resp["total"],
        json!(465),
        "解的筆數跟 cg-pet-calc 的 results.length 不同"
    );
    assert_eq!(
        loss_range(&resp),
        [[0, 4], [0, 4], [0, 4], [0, 4], [2, 4]],
        "掉檔可能解範圍跟機器人不同"
    );
}

/// 機器人印 176（＝真值 186 被扣掉 10）。
#[test]
fn the_bot_s_surfing_duck_comes_out_the_same() {
    let c = Case {
        name: "衝浪小黃鴨",
        grow: [28, 46, 23, 20, 8],
        lvl: 1,
        stat: [114, 77, 50, 40, 31],
    };
    let resp = run(&c);

    assert_eq!(
        resp["total"],
        json!(186),
        "解的筆數跟 cg-pet-calc 的 results.length 不同"
    );
    assert_eq!(
        loss_range(&resp),
        [[0, 4], [0, 4], [0, 4], [0, 4], [1, 4]],
        "掉檔可能解範圍跟機器人不同"
    );
}

/// 機器人印 246（＝真值 256 被扣掉 10）。
///
/// 這隻是七個案例裡**唯一** cg-pet-calc 內建 `PetDefaultData` 也有的
/// —— 本機直接跑 `RealGuess` 得到 `results.length === 256`，
/// 跟這裡的期望值一致，而它自己的格式化輸出印的是 246。扣 10 的證據就是這隻。
#[test]
fn the_bot_s_stone_king_comes_out_the_same() {
    let c = Case {
        name: "石像魔王",
        grow: [12, 36, 13, 18, 46],
        lvl: 1,
        stat: [82, 135, 43, 33, 29],
    };
    let resp = run(&c);

    assert_eq!(
        resp["total"],
        json!(256),
        "解的筆數跟 cg-pet-calc 的 results.length 不同"
    );
    assert_eq!(
        loss_range(&resp),
        [[0, 4], [0, 4], [0, 4], [1, 4], [3, 4]],
        "掉檔可能解範圍跟機器人不同"
    );
}

/// 機器人對這組回「無解」—— 我們也要無解，不能無中生有。
///
/// 群裡當下的說法是「可以確認是否有未點點數或裝備寵物裝備中」，
/// 也就是這串能力本來就不是純檔次算得出來的。
#[test]
fn the_bot_s_unsolvable_case_stays_unsolvable() {
    let c = Case {
        name: "紫翎",
        grow: [20, 6, 13, 45, 40],
        lvl: 29,
        stat: [564, 993, 29, 80, 158],
    };
    let resp = run(&c);
    assert_eq!(resp["total"], json!(0), "機器人說無解，我們卻推出東西來了");
}

/// **總**掉檔範圍，用同一批實測案例對一次。
///
/// 期望值不是抄來的常數 —— 是把 `candidates` 裡每一筆的 `lost_bp` 自己掃一遍算出來的，
/// 拿它跟 `distribution.lost_total_range`（走加權統計那條路）比。兩條路算的是同一件事，
/// 對得起來才表示統計那邊沒有把軸與軸之間的相關性弄丟。
///
/// 只挑筆數 ≤ 200 的案例：`candidates` 有 200 筆上限（`truncated`），
/// 超過的話掃出來的就不是全集，這個對照本身會失效。
#[test]
fn the_total_loss_range_agrees_with_scanning_every_candidate() {
    let cases = [
        Case {
            name: "小白鴨",
            grow: [40, 45, 10, 20, 10],
            lvl: 1,
            stat: [118, 70, 47, 29, 30],
        },
        Case {
            name: "小白鴨",
            grow: [40, 45, 10, 20, 10],
            lvl: 17,
            stat: [481, 291, 182, 71, 68],
        },
        Case {
            name: "寶寶炸彈",
            grow: [18, 40, 10, 48, 9],
            lvl: 17,
            stat: [400, 315, 170, 76, 116],
        },
        Case {
            name: "衝浪小黃鴨",
            grow: [28, 46, 23, 20, 8],
            lvl: 1,
            stat: [114, 77, 50, 40, 31],
        },
    ];

    for c in &cases {
        let resp = run(c);
        assert_eq!(
            resp["truncated"],
            json!(false),
            "{} 的候選被截斷了，這個對照不能用",
            c.name
        );

        let losses: Vec<i64> = resp["candidates"]
            .as_array()
            .expect("沒有候選")
            .iter()
            .map(|r| r["lost_bp"].as_i64().expect("候選沒有 lost_bp"))
            .collect();
        let scanned = json!([
            losses.iter().min().expect("一筆都沒有"),
            losses.iter().max().unwrap()
        ]);

        assert_eq!(
            resp["distribution"]["lost_total_range"], scanned,
            "{} lv{} 的總掉檔範圍，加權統計跟逐筆掃出來的不一樣",
            c.name, c.lvl
        );
    }
}

/// ⭐ 真實資料上，總掉檔範圍**確實推不出來** —— 這就是統計那邊要多存一份的理由。
///
/// 逐軸邊際只說得出「這一軸可能掉幾檔」，各軸最小值的和只是總和的**下界**，
/// 未必真的有哪一組候選同時取到那些值。這裡拿實測案例把差距釘住。
#[test]
fn the_per_axis_bound_is_looser_than_the_real_total() {
    let c = Case {
        name: "衝浪小黃鴨",
        grow: [28, 46, 23, 20, 8],
        lvl: 1,
        stat: [114, 77, 50, 40, 31],
    };
    let resp = run(&c);

    let per_axis = loss_range(&resp);
    let naive: [i32; 2] = [
        per_axis.iter().map(|r| r[0]).sum(),
        per_axis.iter().map(|r| r[1]).sum(),
    ];
    let real = &resp["distribution"]["lost_total_range"];

    assert!(
        real[0].as_i64().unwrap() > naive[0] as i64,
        "逐軸下界 {} 居然就是真正的最小總掉檔 {} —— \
         換一個真的推不出來的案例來測，不然這條測試沒有在測東西",
        naive[0],
        real[0]
    );
}
