//! BP（成長點）→ 實際能力值的正向計算。
//!
//! 五個 BP 軸的順序固定為 `[HP, ATK, DEF, AGI, MP]`（＝ 體力／力量／強度／速度／魔法），
//! 與 `cg-pet-calc` 的 `BP.toArray()` 一致。**不要**改成顯示用的 血魔攻防敏 順序。

/// BP 軸數。
pub const AXES: usize = 5;

/// 血魔攻防敏的基礎值。
pub const PROP_BASE: i64 = 20;
/// 精神／回復的基礎值。
pub const PROP_BASE_WIS_RES: i64 = 100;

/// BP → 能力值的轉換矩陣。每列是一個輸出能力，欄位順序為 `[HP, ATK, DEF, AGI, MP]`。
///
/// 原始註解（cg-pet-calc / 原程式共通）以「輸入軸為列」呈現，這裡是它的轉置：
///
/// ```text
///          生命 魔力 攻擊  防禦  敏捷  精神  回復
/// +體力     8    1   0.2  0.2   0.1  -0.3   0.8
/// +力量     2    2   2.7  0.3   0.2  -0.1  -0.1
/// +強度     3    2   0.3  3     0.2   0.2  -0.1
/// +速度     3    2   0.3  0.3   2    -0.1   0.2
/// +魔法     1   10   0.2  0.2   0.1   0.8  -0.3
/// ```
pub mod matrix {
    pub const HP: [f64; 5] = [8.0, 2.0, 3.0, 3.0, 1.0];
    pub const MP: [f64; 5] = [1.0, 2.0, 2.0, 2.0, 10.0];
    pub const ATK: [f64; 5] = [0.2, 2.7, 0.3, 0.3, 0.2];
    pub const DEF: [f64; 5] = [0.2, 0.3, 3.0, 0.3, 0.2];
    pub const AGI: [f64; 5] = [0.1, 0.2, 0.2, 2.0, 0.1];
    pub const WIS: [f64; 5] = [-0.3, -0.1, 0.2, -0.1, 0.8];
    pub const RES: [f64; 5] = [0.8, -0.1, -0.1, 0.2, -0.3];
}

/// JavaScript `Math.round` 語意：**四捨五入且 .5 一律進位向 +∞**。
///
/// Rust 的 `f64::round()` 是「.5 遠離 0」，對負數與 JS 不同
/// （`Math.round(-0.5) == -0`，但 `(-0.5f64).round() == -1.0`）。
/// 精神／回復的係數有負值，算出來可能是負的，所以這個差異是會踩到的。
#[inline]
pub fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// 對應 cg-pet-calc 的 `fixPos`：先四捨五入到小數第 4 位，再向下取整。
#[inline]
pub fn fix_pos(n: f64) -> i64 {
    (js_round(n * 10000.0) / 10000.0).floor() as i64
}

#[inline]
fn dot(m: &[f64; 5], v: &[f64; 5]) -> f64 {
    m[0] * v[0] + m[1] * v[1] + m[2] * v[2] + m[3] * v[3] + m[4] * v[4]
}

/// 五軸 BP 值（連續量，不是整數檔次）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Bp {
    pub hp: f64,
    pub atk: f64,
    pub def: f64,
    pub agi: f64,
    pub mp: f64,
}

/// 實際能力值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stat {
    pub hp: i64,
    pub mp: i64,
    pub atk: i64,
    pub def: i64,
    pub agi: i64,
    pub wis: i64,
    pub res: i64,
}

impl Bp {
    pub const fn new(hp: f64, atk: f64, def: f64, agi: f64, mp: f64) -> Self {
        Self {
            hp,
            atk,
            def,
            agi,
            mp,
        }
    }

    /// 軸順序 `[HP, ATK, DEF, AGI, MP]`。
    #[inline]
    pub const fn to_array(self) -> [f64; AXES] {
        [self.hp, self.atk, self.def, self.agi, self.mp]
    }

    #[inline]
    pub const fn from_array(a: [f64; AXES]) -> Self {
        Self::new(a[0], a[1], a[2], a[3], a[4])
    }

    #[inline]
    pub fn sum(self) -> f64 {
        self.hp + self.atk + self.def + self.agi + self.mp
    }

    /// 逐軸 `self <= other`，對應 cg-pet-calc 的 `BP.contains`。
    #[inline]
    pub fn contained_in(self, other: Bp) -> bool {
        self.hp <= other.hp
            && self.atk <= other.atk
            && self.def <= other.def
            && self.agi <= other.agi
            && self.mp <= other.mp
    }

    /// 正向計算：BP → 實際能力值。
    pub fn calc_real_num(self) -> Stat {
        let v = self.to_array();
        Stat {
            hp: fix_pos(dot(&matrix::HP, &v)) + PROP_BASE,
            mp: fix_pos(dot(&matrix::MP, &v)) + PROP_BASE,
            atk: fix_pos(dot(&matrix::ATK, &v)) + PROP_BASE,
            def: fix_pos(dot(&matrix::DEF, &v)) + PROP_BASE,
            agi: fix_pos(dot(&matrix::AGI, &v)) + PROP_BASE,
            wis: fix_pos(dot(&matrix::WIS, &v)) + PROP_BASE_WIS_RES,
            res: fix_pos(dot(&matrix::RES, &v)) + PROP_BASE_WIS_RES,
        }
    }
}

impl Stat {
    /// 顯示順序：血 魔 攻 防 敏 精神 回復。
    #[inline]
    pub const fn to_array(self) -> [i64; 7] {
        [
            self.hp, self.mp, self.atk, self.def, self.agi, self.wis, self.res,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_round_matches_javascript_semantics() {
        // 正數與 Rust 一致
        assert_eq!(js_round(0.5), 1.0);
        assert_eq!(js_round(1.4), 1.0);
        assert_eq!(js_round(1.5), 2.0);
        // 負數 .5：JS 進位向 +∞，Rust 內建 round() 會給 -1
        assert_eq!(js_round(-0.5), 0.0);
        assert_eq!(js_round(-1.5), -1.0);
        assert_eq!((-1.5f64).round(), -2.0, "確認與內建 round 真的不同");
        assert_eq!(js_round(-1.6), -2.0);
    }

    #[test]
    fn fix_pos_floors_after_rounding_to_4dp() {
        assert_eq!(fix_pos(3.0), 3);
        assert_eq!(fix_pos(3.9999), 3);
        // 3.99999 -> round4 -> 4.0 -> floor -> 4；這正是 fixPos 存在的理由
        assert_eq!(fix_pos(3.99999), 4);
        assert_eq!(fix_pos(-0.00001), 0);
    }

    /// 全 0 BP 的寵物就是純基礎值。
    #[test]
    fn zero_bp_gives_base_stats() {
        let s = Bp::default().calc_real_num();
        assert_eq!(
            s,
            Stat {
                hp: 20,
                mp: 20,
                atk: 20,
                def: 20,
                agi: 20,
                wis: 100,
                res: 100
            }
        );
    }

    /// 單軸 1.0 BP 應該剛好等於矩陣那一欄（＋基礎值）。
    // `0 + 20` / `100 + 0` 是刻意寫成「矩陣貢獻 + 基礎值」的形式，
    // 化簡掉就看不出這個數字是怎麼來的了。
    #[allow(clippy::identity_op)]
    #[test]
    fn unit_bp_reproduces_matrix_columns() {
        let s = Bp::new(1.0, 0.0, 0.0, 0.0, 0.0).calc_real_num();
        assert_eq!(s.hp, 8 + 20);
        assert_eq!(s.mp, 1 + 20);
        assert_eq!(s.atk, 0 + 20); // 0.2 -> floor 0
        assert_eq!(s.wis, 100 + fix_pos(-0.3)); // -0.3 -> floor -1
        assert_eq!(s.res, 100 + 0); // 0.8 -> floor 0

        let s = Bp::new(0.0, 0.0, 0.0, 0.0, 1.0).calc_real_num();
        assert_eq!(s.mp, 10 + 20);
        assert_eq!(s.hp, 1 + 20);
    }

    #[test]
    fn contains_is_per_axis() {
        let small = Bp::new(1.0, 1.0, 1.0, 1.0, 1.0);
        let big = Bp::new(2.0, 2.0, 2.0, 2.0, 2.0);
        assert!(small.contained_in(big));
        assert!(!big.contained_in(small));
        let mixed = Bp::new(3.0, 1.0, 1.0, 1.0, 1.0);
        assert!(!mixed.contained_in(big));
    }
}
