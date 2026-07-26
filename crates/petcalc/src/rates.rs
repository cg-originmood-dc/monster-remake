//! 成長係數表 `fullRates`。
//!
//! 噬生・魔物觀測者 v3.12 使用的成長係數表，共 111 筆（index 0–110，最大值 4.615）。
//! 111 筆的長度由閉合式（見 `CLAUDE.md` §3.2）以及原程式的配置方式共同確認。
//!
//! ⚠️ `cg-pet-calc` 的同名表只到 index 52，而且 `51` / `52` 的值是**錯的**
//! （寫成 2.04 / 2.08，正確為 2.14 / 2.18）。檔次 > 50 的寵物用該表會算錯，
//! 所以這裡以完整的 111 筆為準（見 `CLAUDE.md` §3.2）。

/// 表的最大 index（含）。
pub const MAX_TIER: usize = 110;

/// 噬生・魔物觀測者 v3.12 的成長係數表，index 即檔次。
pub static FULL_RATES: [f64; MAX_TIER + 1] = [
    0.0, 0.04, 0.08, 0.12, 0.16, 0.205, 0.25, 0.29, 0.33, 0.37, 0.415, 0.46, 0.5, 0.54, 0.58,
    0.625, 0.67, 0.71, 0.75, 0.79, 0.835, 0.88, 0.92, 0.96, 1.0, 1.045, 1.09, 1.13, 1.17, 1.21,
    1.255, 1.3, 1.34, 1.38, 1.42, 1.465, 1.51, 1.55, 1.59, 1.63, 1.675, 1.72, 1.76, 1.8, 1.84,
    1.885, 1.93, 1.97, 2.01, 2.05, 2.095, 2.14, 2.18, 2.22, 2.26, 2.305, 2.35, 2.39, 2.43, 2.47,
    2.515, 2.56, 2.6, 2.64, 2.68, 2.725, 2.77, 2.81, 2.85, 2.89, 2.935, 2.98, 3.02, 3.06, 3.1,
    3.145, 3.19, 3.23, 3.27, 3.31, 3.355, 3.4, 3.44, 3.48, 3.52, 3.565, 3.61, 3.65, 3.69, 3.73,
    3.775, 3.82, 3.86, 3.9, 3.94, 3.985, 4.03, 4.07, 4.11, 4.15, 4.195, 4.24, 4.28, 4.32, 4.36,
    4.405, 4.45, 4.49, 4.53, 4.57, 4.615,
];

/// 查表；超出範圍回傳 `None`（呼叫端自行決定要不要當成 0）。
#[inline]
pub fn full_rate(tier: i32) -> Option<f64> {
    if tier < 0 {
        return None;
    }
    FULL_RATES.get(tier as usize).copied()
}

/// 查表，超出範圍以 0.0 代替 —— 對應 `cg-pet-calc` 的 `fullRates[x] ?? 0` 行為。
#[inline]
pub fn full_rate_or_zero(tier: i32) -> f64 {
    full_rate(tier).unwrap_or(0.0)
}

/// 表的閉合式。
///
/// ```text
/// n = 0        -> 0
/// n > 0        -> 0.04*n + 0.01*floor((n-1)/5) + (n % 5 == 0 ? 0.005 : 0)
/// ```
///
/// 與 dump 出的 111 筆在 1e-9 內完全一致（見 `tests::closed_form_matches_table`），
/// 但**浮點位元不保證相同**，所以正式計算一律走 [`FULL_RATES`] 查表，
/// 這個函式只用於推導／驗證與超出 110 的外插推測。
pub fn full_rate_formula(n: u32) -> f64 {
    if n == 0 {
        return 0.0;
    }
    let n = f64::from(n);
    let step = ((n - 1.0) / 5.0).floor();
    let bonus = if (n as u32) % 5 == 0 { 0.005 } else { 0.0 };
    0.04 * n + 0.01 * step + bonus
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_endpoints() {
        assert_eq!(FULL_RATES.len(), 111);
        assert_eq!(FULL_RATES[0], 0.0);
        assert_eq!(FULL_RATES[1], 0.04);
        assert_eq!(FULL_RATES[MAX_TIER], 4.615);
    }

    /// 回歸測試：釘死 cg-pet-calc 算錯的兩格。
    #[test]
    fn tiers_51_52_are_not_the_cg_pet_calc_values() {
        assert_eq!(FULL_RATES[51], 2.14, "cg-pet-calc 原版誤寫為 2.04");
        assert_eq!(FULL_RATES[52], 2.18, "cg-pet-calc 原版誤寫為 2.08");
    }

    #[test]
    fn closed_form_matches_table() {
        for (n, &expected) in FULL_RATES.iter().enumerate() {
            let got = full_rate_formula(n as u32);
            assert!(
                (got - expected).abs() < 1e-9,
                "index {n}: table={expected} closed-form={got}"
            );
        }
    }

    #[test]
    fn lookup_bounds() {
        assert_eq!(full_rate(-1), None);
        assert_eq!(full_rate(111), None);
        assert_eq!(full_rate_or_zero(111), 0.0);
        assert_eq!(full_rate(110), Some(4.615));
    }
}
