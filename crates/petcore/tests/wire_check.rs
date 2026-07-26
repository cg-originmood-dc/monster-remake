//! 線路契約：釘住幾個指令的**實際回傳數值**。
//!
//! 這不是在測公式（那是 `petcalc` 的事，那邊有跟 cg-pet-calc 逐位元對拍的
//! 1200 筆向量）。這裡測的是**中間任何一層都沒有偷偷改動數字** ——
//! JSON 轉換、軸順序、取整、序列化精度。
//!
//! 底下的期望值是在瀏覽器裡跑 wasm 實測回來的，跟原生 `cargo test` 一字不差。
//! 移植成網頁版時就是靠這組數字確認 wasm 沒有把結果算歪。

use petcore::custom::NullStore;
use petcore::Engine;
use serde_json::{json, Value};

fn engine() -> Engine {
    Engine::new(Box::new(NullStore))
}

/// 檔次 27/16/25/15/37、隨機檔全 2、20 級、完全不加點。
///
/// 這組就是「破曉之刃」的檔次，也是 `petcore` 內部測試在用的素材。
#[test]
fn a_forward_calculation_returns_exactly_these_numbers() {
    let out = engine()
        .dispatch(
            "forward",
            json!({ "req": {
                "grow": {"hp":27,"atk":16,"def":25,"agi":15,"mp":37,"bprate":0.2},
                "lvl": 20, "random": [2,2,2,2,2], "mode": "none",
                "manual": [0,0,0,0,0], "not_order_point": 19, "burst": null
            }}),
        )
        .expect("正算失敗");

    // 能力值是顯示順序 血魔攻防敏（精神回復）
    assert_eq!(
        out["stat"],
        json!({ "hp": 429, "mp": 533, "atk": 89, "def": 118, "agi": 65, "wis": 123, "res": 109 }),
        "能力值變了"
    );

    // BP 是連續量，這裡連浮點尾巴一起釘 —— 取整方式改掉的話邊界會差 1，
    // 而差 1 在推算裡就是「有解」跟「無解」的差別。
    assert_eq!(
        out["bp"],
        json!([27.269999999999996, 16.33, 25.254999999999995, 15.275, 37.25]),
        "BP 變了"
    );
    assert_eq!(out["bp_sum"], json!(121.38));
    assert_eq!(out["manual"], json!([0, 0, 0, 0, 0]));
}

/// 反推回去要找得到原本那組檔次。
#[test]
fn a_guess_finds_the_pet_it_came_from() {
    let mut e = engine();
    let row = e
        .dispatch(
            "forward",
            json!({ "req": {
                "grow": {"hp":27,"atk":16,"def":25,"agi":15,"mp":37,"bprate":0.2},
                "lvl": 20, "random": [2,2,2,2,2], "mode": "none",
                "manual": [0,0,0,0,0], "not_order_point": 19, "burst": null
            }}),
        )
        .unwrap();
    let s = &row["stat"];

    let resp = e
        .dispatch(
            "guess",
            json!({ "req": {
                "grow": {"hp":27,"atk":16,"def":25,"agi":15,"mp":37,"bprate":0.2},
                "target": {"lvl":20,"hp":s["hp"],"mp":s["mp"],"atk":s["atk"],
                           "def":s["def"],"agi":s["agi"]},
                "not_order_point": 19, "calc_mode": "smart", "manual": null,
                "catch_stat": null, "mode": "exact", "exhaustive": true,
                "tolerance": null, "target_grow": null, "limit": 0
            }}),
        )
        .expect("推算失敗");

    let rows = resp["candidates"].as_array().expect("沒有候選欄位");
    assert!(
        rows.iter()
            .any(|c| c["grow"] == json!([27, 16, 25, 15, 37])),
        "{} 筆解裡沒有原本那組檔次",
        rows.len()
    );
    assert_eq!(resp["mode"], json!("exact"));
}

/// 「檔次範圍／隨機檔範圍」是後來才加的欄位，**不傳它時行為必須跟以前一樣**。
///
/// 這條測試是為了那個相容性寫的：`ranges` 有 serde default，省略掉應該
/// 跟明寫預設值得到**逐位元相同**的回應。不然舊的前端一升級就會靜默地
/// 換掉搜尋範圍。
#[test]
fn omitting_the_search_ranges_matches_spelling_out_the_defaults() {
    let base = json!({
        "grow": {"hp":27,"atk":16,"def":25,"agi":15,"mp":37,"bprate":0.2},
        "target": {"lvl":20,"hp":429,"mp":533,"atk":89,"def":118,"agi":65},
        "not_order_point": 19, "calc_mode": "smart", "mode": "exact",
        "exhaustive": true, "limit": 0
    });

    let mut with_defaults = base.clone();
    with_defaults["ranges"] = json!({
        "loss_min":   [0, 0, 0, 0, 0],
        "loss_max":   [4, 4, 4, 4, 4],
        "random_min": [0, 0, 0, 0, 0],
        "random_max": [10, 10, 10, 10, 10],
    });

    let omitted = engine()
        .dispatch("guess", json!({ "req": base }))
        .expect("推算失敗");
    let spelled = engine()
        .dispatch("guess", json!({ "req": with_defaults }))
        .expect("推算失敗");

    assert_eq!(omitted, spelled, "省略 ranges 跟明寫預設值結果不同");
}

/// 收緊「檔次範圍」要真的把解濾掉 —— 不然這個欄位等於沒接上。
#[test]
fn tightening_the_loss_range_removes_the_undropped_answer() {
    let req = |ranges: Value| {
        json!({ "req": {
            "grow": {"hp":27,"atk":16,"def":25,"agi":15,"mp":37,"bprate":0.2},
            "target": {"lvl":20,"hp":429,"mp":533,"atk":89,"def":118,"agi":65},
            "not_order_point": 19, "calc_mode": "smart", "mode": "exact",
            "exhaustive": true, "limit": 0, "ranges": ranges
        }})
    };
    let has_catalog_grow = |v: &Value| {
        v["candidates"]
            .as_array()
            .is_some_and(|r| r.iter().any(|c| c["grow"] == json!([27, 16, 25, 15, 37])))
    };

    let wide = engine()
        .dispatch(
            "guess",
            req(json!({"loss_min":[0,0,0,0,0],"loss_max":[4,4,4,4,4],
                                      "random_min":[0,0,0,0,0],"random_max":[10,10,10,10,10]})),
        )
        .expect("推算失敗");
    assert!(has_catalog_grow(&wide), "預設範圍下應該找得到沒掉檔的那組");

    // 每軸至少掉 1 檔 —— 沒掉檔的那組就不該再出現
    let narrow = engine()
        .dispatch(
            "guess",
            req(json!({"loss_min":[1,1,1,1,1],"loss_max":[4,4,4,4,4],
                                      "random_min":[0,0,0,0,0],"random_max":[10,10,10,10,10]})),
        )
        .expect("推算失敗");
    assert!(!has_catalog_grow(&narrow), "下界 1 之後不該還有沒掉檔的解");
}

/// 隨機檔範圍填到湊不出總和 10 時要**講出來**，不能假裝成「查無解」。
#[test]
fn a_contradictory_random_range_is_reported_not_silently_empty() {
    let err = engine()
        .dispatch(
            "guess",
            json!({ "req": {
                "grow": {"hp":27,"atk":16,"def":25,"agi":15,"mp":37,"bprate":0.2},
                "target": {"lvl":20,"hp":429,"mp":533,"atk":89,"def":118,"agi":65},
                "not_order_point": 19, "calc_mode": "smart", "mode": "exact",
                "exhaustive": true, "limit": 0,
                // 下界合計 15 > 10，永遠湊不出來
                "ranges": {"loss_min":[0,0,0,0,0],"loss_max":[4,4,4,4,4],
                           "random_min":[3,3,3,3,3],"random_max":[10,10,10,10,10]}
            }}),
        )
        .expect_err("條件矛盾卻沒有回報錯誤");

    assert!(err.contains("15"), "錯誤訊息要講出下界合計是多少：{err}");
}

/// 能力搜尋的回傳形狀 —— 前端 `HitDetail` 是直接照這些鍵取值的。
///
/// 用內建圖鑑跑，所以筆數會隨圖鑑變動，這裡只釘**形狀**與**排序不變量**。
/// 數學本身由 `petcalc` 的 15 條測試守著。
#[test]
fn a_stat_search_comes_back_in_the_shape_the_finder_expects() {
    let out = engine()
        .dispatch(
            "search_by_stats",
            // 血 魔 攻 防 敏 精神 回復
            json!({ "req": { "lvl": 50, "floor": [400, 300, 0, 0, 0, 0, 0] } }),
        )
        .expect("搜尋失敗");

    assert_eq!(out["available"], json!(49), "可用點數就是 等級-1");
    let hits = out["hits"].as_array().expect("沒有 hits 欄位");
    assert!(!hits.is_empty(), "50 級竟然一隻都補不到血 400 魔 300");
    assert_eq!(out["total"], json!(hits.len()), "沒設 limit 就不該被截斷");
    assert_eq!(out["truncated"], json!(false));

    for h in hits {
        // 前端取的是 hit.pet.name / hit.stat.hp / hit.needed / hit.spare
        assert!(h["pet"]["name"].is_string(), "{h}");
        assert!(h["stat"]["hp"].is_i64(), "{h}");
        assert!(h["needed"].is_i64() && h["spare"].is_i64(), "{h}");
        // 命中的定義就是「補得完」，所以 needed 一定花得起
        assert!(h["needed"].as_i64().unwrap() <= 49, "{h}");
    }

    // 餘裕多的排前面 —— 這是清單的唯一排序保證
    let spares: Vec<i64> = hits.iter().map(|h| h["spare"].as_i64().unwrap()).collect();
    assert!(
        spares.windows(2).all(|w| w[0] >= w[1]),
        "剩餘點數沒有由多到少排"
    );
}

/// 輸出的 JSON 必須是普通物件，不是 JS 的 `Map`。
///
/// `serde-wasm-bindgen` 預設會把 map 序列化成 `Map`，前端 `row.stat.hp`
/// 就會拿到 `undefined`。wasm 那邊靠 `Serializer::json_compatible()` 擋掉；
/// 這裡在原生端釘住「該是物件的地方就是物件、該是陣列的就是陣列」。
#[test]
fn responses_are_plain_objects_all_the_way_down() {
    let out = engine()
        .dispatch(
            "forward",
            json!({ "req": {
                "grow": {"hp":1,"atk":1,"def":1,"agi":1,"mp":1,"bprate":0.2},
                "lvl": 1, "random": [2,2,2,2,2], "mode": "none",
                "manual": null, "not_order_point": 0, "burst": null
            }}),
        )
        .unwrap();
    assert!(matches!(out, Value::Object(_)));
    assert!(matches!(out["stat"], Value::Object(_)));
    assert!(matches!(out["bp"], Value::Array(_)));
}
