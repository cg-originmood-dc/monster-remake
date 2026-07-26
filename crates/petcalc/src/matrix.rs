//! 能力值 ↔ BP 的線性代數。
//!
//! 正向是 `stat = M · bp`（見 [`crate::bp::matrix`]）。反推一隻寵物的 BP 需要解
//! `M · bp = stat - 20`，`cg-pet-calc` 用 mathjs 的 `lusolve`，這裡自己寫一個
//! 5×5 部分軸選主元的高斯消去，避免拉進矩陣函式庫。
//!
//! 另外保留 `cg-pet-calc` 內嵌的 [`M_INV`]（四捨五入到小數 8 位的反矩陣）。
//! 它**只**用在推算時的代數初猜，不用於任何需要精確度的地方 ——
//! 精確解一律走 [`lu_solve`]。

use crate::bp::{matrix as fwd, Bp, AXES, PROP_BASE};

/// 正向矩陣。列 = 輸出能力 `[生命, 魔力, 攻擊, 防禦, 敏捷]`，
/// 欄 = BP 軸 `[HP, ATK, DEF, AGI, MP]`。
///
/// 精神／回復不在其中：它們由同一組 BP 決定，但**不是**獨立自由度，
/// 放進來會讓系統超定。
pub const M_BP: [[f64; AXES]; AXES] = [fwd::HP, fwd::MP, fwd::ATK, fwd::DEF, fwd::AGI];

/// `M_BP` 的反矩陣，取自 `cg-pet-calc`（小數 8 位）。
///
/// 列 = BP 軸 `[HP, ATK, DEF, AGI, MP]`，欄 = 能力 `[生命, 魔力, 攻擊, 防禦, 敏捷]`。
///
/// ⚠️ 這是**近似值**。誤差約 1e-8，拿來當推算的起始猜測綽綽有餘，
/// 但不要用它取代 [`lu_solve`]。
pub const M_INV: [[f64; AXES]; AXES] = [
    [
        0.13249019,
        -0.00806319,
        -0.06785930,
        -0.10938981,
        -0.16408472,
    ],
    [
        -0.00782811,
        -0.00608853,
        0.38607709,
        -0.02429143,
        -0.03643715,
    ],
    [
        -0.00695832,
        -0.00541203,
        -0.02719073,
        0.34877798,
        -0.03238858,
    ],
    [
        -0.00467806,
        -0.00363849,
        -0.02954152,
        -0.02452651,
        0.51876579,
    ],
    [
        -0.00935612,
        0.10383413,
        -0.05908304,
        -0.04905303,
        -0.07357954,
    ],
];

/// 解 `a · x = b`（5×5，部分軸選主元）。奇異矩陣回傳 `None`。
// 消去迴圈裡的 c 同時索引 m[r] 與 m[col] 兩列，改寫成迭代器要先 split_at_mut，
// 為了 5×5 的矩陣不值得。
#[allow(clippy::needless_range_loop)]
pub fn lu_solve(a: [[f64; AXES]; AXES], b: [f64; AXES]) -> Option<[f64; AXES]> {
    let mut m = a;
    let mut y = b;

    for col in 0..AXES {
        // 選主元：這一欄絕對值最大的列
        let (pivot, _) = (col..AXES)
            .map(|r| (r, m[r][col].abs()))
            .max_by(|x, z| x.1.partial_cmp(&z.1).unwrap_or(std::cmp::Ordering::Equal))?;
        if m[pivot][col].abs() < 1e-12 {
            return None;
        }
        m.swap(col, pivot);
        y.swap(col, pivot);

        for r in (col + 1)..AXES {
            let f = m[r][col] / m[col][col];
            if f == 0.0 {
                continue;
            }
            for c in col..AXES {
                m[r][c] -= f * m[col][c];
            }
            y[r] -= f * y[col];
        }
    }

    // 回代
    let mut x = [0.0f64; AXES];
    for r in (0..AXES).rev() {
        let mut acc = y[r];
        for c in (r + 1)..AXES {
            acc -= m[r][c] * x[c];
        }
        x[r] = acc / m[r][r];
    }
    Some(x)
}

/// 由觀測到的五項能力反推 BP —— 對應 `cg-pet-calc` 的 `Stat.toBP()`。
///
/// 參數順序是**顯示順序** 血魔攻防敏，回傳的 [`Bp`] 則是 BP 軸順序。
/// 這個轉換必定有唯一解（`M_BP` 可逆），但因為輸入是取整過的整數能力，
/// 解出來的 BP 只是「落在該整數格內的某一點」，不是真值。
pub fn stat_to_bp(hp: i64, mp: i64, atk: i64, def: i64, agi: i64) -> Bp {
    let b = [
        (hp - PROP_BASE) as f64,
        (mp - PROP_BASE) as f64,
        (atk - PROP_BASE) as f64,
        (def - PROP_BASE) as f64,
        (agi - PROP_BASE) as f64,
    ];
    // M_BP 是常數且可逆，lu_solve 不會失敗
    Bp::from_array(lu_solve(M_BP, b).expect("M_BP 必定可逆"))
}

/// `M_INV · v`，用於推算的代數初猜。
#[inline]
pub fn apply_inv(v: [f64; AXES]) -> [f64; AXES] {
    std::array::from_fn(|r| {
        let row = &M_INV[r];
        row[0] * v[0] + row[1] * v[1] + row[2] * v[2] + row[3] * v[3] + row[4] * v[4]
    })
}

/// `M_BP · bp`，正向但**不取整** —— 推算內圈要拿原始浮點值比對。
#[inline]
pub fn apply_fwd(bp: [f64; AXES]) -> [f64; AXES] {
    std::array::from_fn(|r| {
        let row = &M_BP[r];
        row[0] * bp[0] + row[1] * bp[1] + row[2] * bp[2] + row[3] * bp[3] + row[4] * bp[4]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lu_solve_inverts_the_forward_matrix() {
        // 隨手挑一組 BP，正向算出能力，再解回來
        let bp = [3.5, 1.25, 7.0, 0.5, 2.75];
        let stat = apply_fwd(bp);
        let back = lu_solve(M_BP, stat).unwrap();
        for i in 0..AXES {
            assert!(
                (back[i] - bp[i]).abs() < 1e-9,
                "軸 {i}: {} != {}",
                back[i],
                bp[i]
            );
        }
    }

    #[test]
    fn lu_solve_rejects_singular_input() {
        let mut singular = M_BP;
        singular[1] = singular[0]; // 兩列相同
        assert!(lu_solve(singular, [1.0; AXES]).is_none());
    }

    /// cg-pet-calc 內嵌的 M_INV 真的是 M_BP 的反矩陣（在它的精度內）。
    // i/j 是矩陣的列欄索引，寫成 for i in 0..N 才看得出在驗單位矩陣。
    #[allow(clippy::needless_range_loop)]
    #[test]
    fn m_inv_is_approximately_the_inverse() {
        for i in 0..AXES {
            for j in 0..AXES {
                // (M_INV · M_BP)[i][j] 應該是單位矩陣
                let v: f64 = (0..AXES).map(|k| M_INV[i][k] * M_BP[k][j]).sum();
                let expect = if i == j { 1.0 } else { 0.0 };
                assert!(
                    (v - expect).abs() < 1e-7,
                    "M_INV·M_BP[{i}][{j}] = {v}，應為 {expect}"
                );
            }
        }
    }

    /// M_INV 是近似值，精確解要用 lu_solve —— 這個測試把差距釘出來。
    #[test]
    fn m_inv_is_less_accurate_than_lu_solve() {
        let bp = [30.0, 20.0, 25.0, 15.0, 40.0];
        let stat = apply_fwd(bp);
        let exact = lu_solve(M_BP, stat).unwrap();
        let approx = apply_inv(stat);

        let err_exact: f64 = (0..AXES)
            .map(|i| (exact[i] - bp[i]).abs())
            .fold(0.0, f64::max);
        let err_approx: f64 = (0..AXES)
            .map(|i| (approx[i] - bp[i]).abs())
            .fold(0.0, f64::max);
        assert!(err_exact < 1e-10);
        assert!(
            err_approx > err_exact,
            "M_INV 竟然比 lu_solve 準？{err_approx} vs {err_exact}"
        );
    }

    #[test]
    fn stat_to_bp_round_trips_through_calc_real_num() {
        let bp = Bp::new(12.0, 4.0, 6.0, 3.0, 8.0);
        let s = bp.calc_real_num();
        let back = stat_to_bp(s.hp, s.mp, s.atk, s.def, s.agi);
        // 能力被取整過，所以只會落在同一格內，不會完全相等
        for (a, b) in back.to_array().iter().zip(bp.to_array().iter()) {
            assert!((a - b).abs() < 0.5, "{a} vs {b}");
        }
    }
}
