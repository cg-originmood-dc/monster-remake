//! 寵物圖鑑：內建表 ＋ `spetdata.ini` 讀取。
//!
//! 兩個資料來源：
//!
//! * **內建表** —— 由 `tools/gen/gen_petdata.mjs` 從 `cg-pet-calc` 的
//!   `PetDefaultData` 轉出的 TSV，編譯進二進位檔（755 筆）。
//! * **`spetdata.ini`** —— 原程式的自動更新檔（`CLAUDE.md` §2.1），
//!   來源 `https://cg-static.tonyq.org/pet/spetdata.ini`，
//!   **UTF-16-LE with BOM**。有這個檔就用它，沒有就用內建表。
//!
//! 兩者的檔次欄位順序都是 `[HP, ATK, DEF, AGI, MP]`
//! （體力／力量／強度／速度／魔法），與 [`petcalc`] 的軸順序一致。

use std::collections::BTreeMap;

/// 檔次軸數，與 `petcalc::AXES` 相同。
pub const AXES: usize = 5;

/// 種族碼 → 名稱。**名稱以 [`parse_originmood`] 那份上游為準。**
///
/// 碼本身沒動（原程式怎麼編就怎麼編），換的只有顯示名稱 —— 對照關係是拿
/// 1069 筆寵物**逐名 join** 出來的，九個種族一對一、零衝突，測試
/// `the_race_codes_still_mean_what_the_original_meant` 釘住這件事：
///
/// | 碼 | 原程式 | 現用 |
/// |---|---|---|
/// | 0 | 野獸系 | 野獸系 |
/// | 1 | 死靈系 | 不死系 |
/// | 2 | 邪魔系 | 飛行系 |
/// | 3 | 蟲系 | 昆蟲系 |
/// | 4 | 植物系 | 植物系 |
/// | 5 | 能源系 | **特殊系** |
/// | 6 | 各種系 | 金屬系 |
/// | 7 | 龍族系 | 龍系 |
/// | 8 | 人族系 | 人形系 |
///
/// ⚠️ 原程式另有一個 `9 特殊系`，但「特殊系」這個名字在新表是碼 5 的。
/// 碼 9 全部的內容就是 `大公雞(缺)` 與 `死神公雞` 兩隻，兩隻都不在新上游裡，
/// 硬留著只會有兩個同名的分頁。所以碼 9 退休，那兩隻併進「其他」。
///
/// `9 其他` 不是遊戲裡的種族，是給**沒有種族資訊**的資料用的
/// —— 內建表有 483 筆這種（見 [`RACE_UNKNOWN`]）。原程式的寵物檔案
/// 視窗本來就有「其他」這個分頁，正好收容它們。
pub const RACE_NAMES: [&str; 10] = [
    "野獸系",
    "不死系",
    "飛行系",
    "昆蟲系",
    "植物系",
    "特殊系",
    "金屬系",
    "龍系",
    "人形系",
    "其他",
];

/// 遊戲種族碼的上限（`0..=8`）—— [`RACE_UNKNOWN`] 不算遊戲種族。
pub const MAX_RACE: u8 = 8;

/// 種族未知。
pub const RACE_UNKNOWN: u8 = 9;

/// 名稱 → 種族碼。認不得就回 [`RACE_UNKNOWN`]。
pub fn race_code(name: &str) -> u8 {
    RACE_NAMES
        .iter()
        .position(|n| *n == name.trim())
        .map_or(RACE_UNKNOWN, |i| i as u8)
}

/// 檔次上限，與 `petcalc::MAX_TIER` 一致。
///
/// 這裡重寫一份而不是相依 `petcalc`，是為了讓 `petdata` 維持零相依；
/// 兩邊對不上會被 `src-tauri` 的 `tier_bounds_agree_with_the_engine` 測試抓到。
pub const MAX_TIER: i32 = 110;

/// 原程式寵物檔案視窗的分頁順序（`CLAUDE.md` §4.4）。
///
/// 順序照抄原程式（野獸 特殊 死靈 能源 邪魔 植物 人族 蟲 龍族 各種 其他），
/// 只少了退休的碼 9 —— 換上新名字之後讀起來是
/// 野獸系 不死系 特殊系 飛行系 植物系 人形系 昆蟲系 龍系 金屬系 其他。
pub const RACE_TAB_ORDER: [u8; 10] = [0, 1, 5, 2, 4, 8, 3, 7, 6, RACE_UNKNOWN];

pub fn race_name(race: u8) -> &'static str {
    RACE_NAMES.get(race as usize).copied().unwrap_or("其他")
}

/// 一隻圖鑑寵物。
#[derive(Debug, Clone, PartialEq)]
pub struct Pet {
    /// 遊戲內的寵物編號。內建表有 483 筆沒有編號（見 [`builtin`]），
    /// 所以是 `Option` —— 編號是動畫資源（`C{N}`）的鍵，
    /// 拿不到就是拿不到，不該編一個假的出來。
    pub id: Option<i32>,
    pub name: String,
    pub race: u8,
    /// 五項成長檔次，順序 `[HP, ATK, DEF, AGI, MP]`。
    pub grow: [i32; AXES],
    /// 能力倍率。INI 裡存的是 ×100 的整數（恆為 20 → 0.20）。
    pub bprate: f64,
    /// 可學技能格數（`spetdata.ini` 第 11 欄，意義未完全確認，實測範圍 6–10）。
    pub skills: Option<i32>,
    /// 地／水／火／風四屬性，總和 100。內建表沒有這欄。
    pub element: Option<[i32; 4]>,
    /// 這筆是不是使用者自己加的 —— 也就是能不能改、能不能刪。
    ///
    /// 原程式劃的是同一條線：`[自製寵物]` 段的記錄可以編輯並寫回 INI，
    /// 內建表則是原程式的寵物資料初始化把整份資料當字串常數寫死在
    /// 執行檔裡，沒有任何路徑可以改它。這裡沿用這個分界。
    pub custom: bool,
}

/// [`Pet::validate`] 的失敗原因。
///
/// 原程式幾乎不驗（只擋空欄位、把能力倍率夾到 0..=10），使用者打什麼就寫什麼。
/// 這裡驗嚴一點：壞掉的檔次會讓推算算出一堆無意義的解，當下卻看不出是資料的錯。
#[derive(Debug, Clone, PartialEq)]
pub enum PetError {
    EmptyName,
    /// 檔次超出 `0..=`[`MAX_TIER`]。
    TierOutOfRange {
        axis: usize,
        value: i32,
    },
    /// 種族碼超出 `0..=`[`RACE_UNKNOWN`]。
    BadRace(u8),
    /// 能力倍率不是正數。原程式的欄位旁邊就寫著「※不須改動」，恆為 20（＝0.20）。
    BadRate(f64),
    /// 地水火風填了但加起來不是 100。
    ElementNotHundred(i32),
}

impl std::fmt::Display for PetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PetError::EmptyName => write!(f, "魔物名字不能空白"),
            PetError::TierOutOfRange { axis, value } => {
                let name = ["體力", "力量", "強度", "速度", "魔法"][*axis];
                write!(f, "{name}檔次 {value} 超出 0..={MAX_TIER}")
            }
            PetError::BadRace(r) => write!(f, "種族碼 {r} 超出 0..={RACE_UNKNOWN}"),
            PetError::BadRate(v) => write!(f, "能力倍率 {v} 必須是正數（原程式恆為 0.20）"),
            PetError::ElementNotHundred(sum) => {
                write!(f, "地水火風加起來是 {sum}，應該要 100")
            }
        }
    }
}

impl std::error::Error for PetError {}

impl Pet {
    /// 寫進圖鑑前的檢查。
    ///
    /// 只擋「會讓後續計算變成垃圾」的東西 —— 名字、檔次範圍、種族碼、倍率，
    /// 以及地水火風的總和。技能格不驗：那一欄的意義本來就沒完全確認
    /// （`CLAUDE.md` §2.1 第 11 欄），拿不準的東西不該擋使用者。
    pub fn validate(&self) -> Result<(), PetError> {
        if self.name.trim().is_empty() {
            return Err(PetError::EmptyName);
        }
        for (axis, &g) in self.grow.iter().enumerate() {
            if !(0..=MAX_TIER).contains(&g) {
                return Err(PetError::TierOutOfRange { axis, value: g });
            }
        }
        if self.race > RACE_UNKNOWN {
            return Err(PetError::BadRace(self.race));
        }
        // is_finite 這一半是擋 NaN 的：NaN 的所有比較都是 false，
        // 只寫 `<= 0.0` 會讓它整個溜過去，然後在推算裡把每個能力值都算成 NaN。
        if !self.bprate.is_finite() || self.bprate <= 0.0 {
            return Err(PetError::BadRate(self.bprate));
        }
        if let Some(e) = self.element {
            let sum: i32 = e.iter().sum();
            if sum != 100 {
                return Err(PetError::ElementNotHundred(sum));
            }
        }
        Ok(())
    }
}

/// 一份圖鑑。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Catalog {
    pub pets: Vec<Pet>,
    /// 資料來源說明，用於介面上顯示「目前用的是哪份表」。
    pub source: String,
}

impl Catalog {
    pub fn by_race(&self, race: u8) -> Vec<&Pet> {
        self.pets.iter().filter(|p| p.race == race).collect()
    }

    /// 名稱子字串搜尋。
    pub fn search(&self, keyword: &str) -> Vec<&Pet> {
        let k = keyword.trim();
        if k.is_empty() {
            return Vec::new();
        }
        self.pets.iter().filter(|p| p.name.contains(k)).collect()
    }

    pub fn find_by_name(&self, name: &str) -> Option<&Pet> {
        self.pets.iter().find(|p| p.name == name)
    }

    pub fn find_by_id(&self, id: i32) -> Option<&Pet> {
        self.pets.iter().find(|p| p.id == Some(id))
    }

    /// 疊上自製寵物：**同名的換掉、沒有的接在後面**。
    ///
    /// 用名字當鍵而不是編號，因為內建表有 483 筆根本沒有編號
    /// （見 [`builtin`]），而名字是使用者在主視窗實際拿來挑寵物的東西。
    ///
    /// 「換掉」這條就是覆寫：改了一隻內建寵物的檔次之後，圖鑑上那隻會變成
    /// 使用者的版本，把自製那筆刪掉就會露出原本的。內建表本身一個位元組都不會動
    /// —— 它是編進二進位檔的唯讀資料。
    pub fn overlay(&mut self, custom: &[Pet]) {
        for pet in custom {
            let mut pet = pet.clone();
            pet.custom = true;
            match self.pets.iter().position(|p| p.name == pet.name) {
                Some(i) => self.pets[i] = pet,
                None => self.pets.push(pet),
            }
        }
    }

    /// 疊上另一份**上游**表：同名的以 `newer` 為準，沒有的接在後面。
    ///
    /// 跟 [`Catalog::overlay`] 的差別只有一個 —— 這裡不會把 `custom` 標成 true。
    /// 兩份上游揉在一起還是上游，使用者不該能編輯或刪除它們。
    ///
    /// 用途是把線上那份（[`parse_originmood`]，1069 筆、資料完整）疊在內建表上。
    /// 兩邊不是包含關係：線上那份有 587 隻內建表沒有的，內建表也有 261 隻
    /// 線上那份沒有的舊寵。取聯集才不會讓任何一隻查不到。
    /// 編號是唯一的例外：`newer` 沒有編號時**留著舊的那個**。
    /// 編號是原程式動畫資源（`C{N}`）的鍵，新上游不提供不代表它不存在。
    pub fn merge_upstream(&mut self, newer: &[Pet]) {
        for pet in newer {
            match self.pets.iter().position(|p| p.name == pet.name) {
                Some(i) => {
                    let id = pet.id.or(self.pets[i].id);
                    self.pets[i] = Pet { id, ..pet.clone() };
                }
                None => self.pets.push(pet.clone()),
            }
        }
    }
}

const BUILTIN_TSV: &str = include_str!("../data/builtin_pets.tsv");

/// 內建圖鑑（編譯進二進位檔）。
///
/// ⚠️ 上游的 `PetDefaultData` 有**兩種列形狀**，`gen_petdata.mjs` 原樣轉出：
///
/// | | 筆數 | 編號 | 種族 | 技能格 |
/// |---|---|---|---|---|
/// | 完整列 | 272 | 有 | 有 | 有 |
/// | 精簡列 | 483 | **空** | **空** | **空** |
///
/// 精簡列是後來補進上游的社群資料，只有名字跟五項檔次。缺的欄位在 TSV 裡是
/// 空字串，這裡轉成 `None` / [`RACE_UNKNOWN`]，**不會**因此丟掉整列
/// —— 檔次才是這個程式要用的東西，而精簡列的檔次是齊全的。
/// 使用者載入 `spetdata.ini` 之後就會拿到完整的種族與屬性。
pub fn builtin() -> Catalog {
    let mut pets = Vec::new();
    for line in BUILTIN_TSV.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 10 {
            continue;
        }
        // 空欄位＝資料本來就沒有，不是格式錯誤
        let opt = |i: usize| f[i].trim().parse::<i32>().ok();
        let Some(grow) = parse_five(&f[3..8]) else {
            continue;
        };
        let Ok(bprate) = f[8].parse::<f64>() else {
            continue;
        };
        if f[1].is_empty() {
            continue;
        }
        pets.push(Pet {
            id: opt(0),
            name: f[1].to_string(),
            race: opt(2).map_or(RACE_UNKNOWN, |r| r.clamp(0, MAX_RACE as i32) as u8),
            grow,
            bprate,
            skills: opt(9),
            element: None,
            custom: false,
        });
    }
    Catalog {
        pets,
        source: "內建表（cg-pet-calc PetDefaultData）".into(),
    }
}

fn parse_five(f: &[&str]) -> Option<[i32; AXES]> {
    if f.len() < AXES {
        return None;
    }
    let mut out = [0; AXES];
    for (o, s) in out.iter_mut().zip(f) {
        *o = s.trim().parse().ok()?;
    }
    Some(out)
}

/// 解 `spetdata.ini` 的位元組。
///
/// ⚠️ **這條路已經不在執行路徑上了。** 圖鑑來源換成 originmood 的 CSV
/// （[`parse_originmood`]），INI 只剩兩個用途：一是 `CLAUDE.md` §2.1 記著那個格式，
/// 二是誰手上有原程式安裝目錄的 `spetdata.ini` 時，這是唯一讀得回來的東西。
/// 所以留著，但沒有人呼叫它 —— 底下那些測試是它僅存的使用者。
///
/// 自動判別編碼：UTF-16-LE BOM（`FF FE`，正規格式）、UTF-8 BOM，或當成 UTF-8。
/// 無法解碼的位元組會被替換 —— `spetdata.ini` 檔尾的 `[龍的砂時計]` 區段
/// 在某些版本是 cp950，但那兩行只有備份日期，不影響寵物資料。
///
/// 記錄格式（`CLAUDE.md` §2.1）：
///
/// ```ini
/// [自製寵物]
/// 編號100=破曉之刃,27 16 25 15 37 20 6 0 0 40 60 8
/// ```
///
/// 12 個空白分隔整數依序是：
/// 體力 力量 強度 速度 魔法 能力倍率 種族 地 水 火 風 技能格。
pub fn parse_spetdata(bytes: &[u8]) -> Result<Catalog, ParseError> {
    let text = decode(bytes);
    let mut pets = Vec::new();
    // 同一個編號在檔案裡只會出現一次，但用 map 可以擋掉重複載入
    let mut seen: BTreeMap<i32, usize> = BTreeMap::new();
    let mut in_pets = false;
    let mut bad = 0usize;

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(section) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            in_pets = section == "自製寵物";
            continue;
        }
        if !in_pets {
            continue;
        }
        let Some(rest) = line.strip_prefix("編號") else {
            continue;
        };
        let Some((id_str, body)) = rest.split_once('=') else {
            bad += 1;
            continue;
        };
        let Ok(id) = id_str.trim().parse::<i32>() else {
            bad += 1;
            continue;
        };
        let Some((name, nums)) = body.split_once(',') else {
            bad += 1;
            continue;
        };
        let f: Vec<&str> = nums.split_whitespace().collect();
        if f.len() < 12 {
            bad += 1;
            continue;
        }
        let Some(grow) = parse_five(&f[0..5]) else {
            bad += 1;
            continue;
        };
        let parse = |s: &str| s.parse::<i32>().unwrap_or(0);
        let pet = Pet {
            id: Some(id),
            name: name.trim().to_string(),
            race: parse(f[6]).clamp(0, MAX_RACE as i32) as u8,
            grow,
            // 第 6 欄是 ×100 的能力倍率（恆為 20）；0 顯然是壞資料，退回預設
            bprate: match parse(f[5]) {
                0 => 0.2,
                v => v as f64 / 100.0,
            },
            skills: skill_slots(parse(f[11])),
            element: Some([parse(f[7]), parse(f[8]), parse(f[9]), parse(f[10])]),
            // spetdata.ini 是原程式自動更新下來的上游資料，不是使用者自己編的
            custom: false,
        };
        match seen.get(&id) {
            Some(&i) => pets[i] = pet,
            None => {
                seen.insert(id, pets.len());
                pets.push(pet);
            }
        }
    }

    if pets.is_empty() {
        return Err(ParseError::NoPets { bad_lines: bad });
    }
    Ok(Catalog {
        pets,
        source: format!("spetdata.ini（{} 筆）", seen.len()),
    })
}

/// 解 originmood 圖鑑的 CSV（**現行的上游**）。
///
/// 來源：`cg-originmood-dc/cg-originmood-dc.github.io` 的
/// `content/data/專屬寵物.csv`，UTF-8、有表頭、1069 筆。
///
/// ```csv
/// 名稱,種族,體力,力量,防禦,速度,魔法,技格,總檔,屬性,技能,image,任務用途
/// 奧菲兒,人形系,43,6,6,20,50,9,125,地10,超強石化魔法LV10、…,/img/…,
/// ```
///
/// 比 `spetdata.ini` 好的地方：種族是**名字**不是碼（所以不必猜對照）、
/// 技能格與檔次總和分成兩欄（`spetdata.ini` 只有一欄而且世代之間換過意義，
/// 見 [`skill_slots`]）、而且多了技能名稱與圖片路徑。
///
/// 欄位**照表頭找，不照位置**：上游哪天在中間插一欄不該讓整份資料錯位。
/// 沒有引號欄位（實測 0 個），所以逗號切就夠了。
///
/// 能力倍率這份沒有 —— 原程式那欄恆為 20（＝0.20），
/// 欄位旁邊就寫著「※不須改動」，所以直接給 [`DEFAULT_BPRATE`]。
pub fn parse_originmood(text: &str) -> Result<Catalog, ParseError> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let Some(header) = lines.next() else {
        return Err(ParseError::NoPets { bad_lines: 0 });
    };
    // 表頭第一欄可能帶 UTF-8 BOM
    let cols: Vec<&str> = header.trim_start_matches('\u{feff}').split(',').collect();
    let at = |name: &str| cols.iter().position(|c| c.trim() == name);
    let (Some(i_name), Some(i_race), Some(i_slots), Some(i_elem)) =
        (at("名稱"), at("種族"), at("技格"), at("屬性"))
    else {
        return Err(ParseError::NoPets { bad_lines: 0 });
    };
    let Some(grow_cols) = ["體力", "力量", "防禦", "速度", "魔法"]
        .iter()
        .map(|c| at(c))
        .collect::<Option<Vec<_>>>()
    else {
        return Err(ParseError::NoPets { bad_lines: 0 });
    };

    let mut pets = Vec::new();
    let mut bad = 0usize;
    for line in lines {
        let f: Vec<&str> = line.split(',').collect();
        let get = |i: usize| f.get(i).map(|s| s.trim()).unwrap_or("");
        let name = get(i_name);
        if name.is_empty() {
            bad += 1;
            continue;
        }
        let raw_grow: Vec<&str> = grow_cols.iter().map(|&i| get(i)).collect();
        let Some(grow) = parse_five(&raw_grow) else {
            bad += 1;
            continue;
        };
        pets.push(Pet {
            // CSV 沒有編號欄。編號是原程式動畫資源的鍵，這份上游不提供
            // —— 疊在內建表上時，`merge_upstream` 會把內建那筆的編號留著。
            id: None,
            name: name.to_string(),
            race: race_code(get(i_race)),
            grow,
            bprate: DEFAULT_BPRATE,
            skills: skill_slots(get(i_slots).parse().unwrap_or(0)),
            element: parse_elements(get(i_elem)),
            custom: false,
        });
    }

    if pets.is_empty() {
        return Err(ParseError::NoPets { bad_lines: bad });
    }
    let n = pets.len();
    Ok(Catalog {
        pets,
        source: format!("originmood 專屬寵物（{n} 筆）"),
    })
}

/// 能力倍率的預設值。原程式那一欄恆為 20（×100 表示），旁邊寫著「※不須改動」。
pub const DEFAULT_BPRATE: f64 = 0.2;

/// 解屬性字串：`地10`、`火9風1`、`風1地9` → `[地, 水, 火, 風]`，總和 100。
///
/// 上游是以 10 為滿，這裡乘 10 換成跟 `spetdata.ini` 一樣的百分比，
/// 讓兩個來源的 [`Pet::element`] 是同一個單位。
///
/// ⚠️ 上游有一筆髒資料：`暴雪魔獸` 是 `水10(水5火5)`，加起來 20。
/// 括號後面看起來是變身或別的型態，語意沒查證 —— 所以只取**括號前**那段，
/// 而不是整串加總（加總會得到 200，被 [`Pet::validate`] 擋掉，那隻就整個消失）。
/// 同一隻在 `spetdata.ini` 裡也是髒的（`0 100 50 0`，加起來 150）。
fn parse_elements(s: &str) -> Option<[i32; 4]> {
    let head = s.split('(').next().unwrap_or("").trim();
    if head.is_empty() {
        return None;
    }
    let mut out = [0; 4];
    let mut chars = head.chars().peekable();
    while let Some(c) = chars.next() {
        let axis = match c {
            '地' => 0,
            '水' => 1,
            '火' => 2,
            '風' => 3,
            _ => return None,
        };
        let mut n = String::new();
        while chars.peek().is_some_and(char::is_ascii_digit) {
            n.push(chars.next().unwrap());
        }
        out[axis] += n.parse::<i32>().ok()? * 10;
    }
    (out.iter().sum::<i32>() == 100).then_some(out)
}

/// 技能格數的上限。超過就代表**那一欄不是技能格數**——見 [`skill_slots`]。
pub const MAX_SKILL_SLOTS: i32 = 20;

/// `spetdata.ini` 第 12 欄（index 11）的解讀。
///
/// ⚠️ **這一欄的意義在上游換過。** 手上四份檔案分成兩個世代：
///
/// | 來源 | 筆數 | 第 12 欄範圍 |
/// |---|---|---|
/// | 2023 年版 | 499 | 6..10 |
/// | v3.12 安裝目錄的 `spetdata.ini` | 791 | 6..10 |
/// | v3.12 安裝目錄的 `2spetdata.ini` | 522 | 6..10 |
/// | `cg-static.tonyq.org/pet/spetdata.ini`（現行） | 1069 | **50..127** |
///
/// ### ✅ 已結案：新世代那一欄是「總檔」不是技能格
///
/// originmood 那份 CSV（[`parse_originmood`]）把兩件事**分成兩欄**：
/// `技格` 落在 6..10、`總檔` 落在 50..127。拿它跟現行 `spetdata.ini` 逐名對，
/// **1069/1069 筆的第 12 欄等於 `總檔`、0 筆等於 `技格`** ——
/// 所以新世代那一欄是五項檔次的和，舊世代的 6..10 才是技能格數。
///
/// 這也解釋了為什麼當初數出「1047/1069 剛好等於檔次和」而不是全部：
/// 上游自己有 22 筆 `總檔` 跟五項和對不起來的髒資料。
///
/// 結論是這個函式**不用改**：超過 [`MAX_SKILL_SLOTS`] 就回 `None` 這個保守作法
/// 剛好把新世代的總檔擋在外面，沒讓介面顯示過「技能格 125」。
pub fn skill_slots(v: i32) -> Option<i32> {
    (1..=MAX_SKILL_SLOTS).contains(&v).then_some(v)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// 沒讀到任何寵物 —— 通常是編碼判斷錯誤或根本不是 spetdata.ini。
    NoPets { bad_lines: usize },
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::NoPets { bad_lines } => {
                write!(
                    f,
                    "spetdata.ini 裡沒有讀到任何寵物（{bad_lines} 行格式不符）"
                )
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// BOM 嗅探式解碼。
fn decode(bytes: &[u8]) -> String {
    match bytes {
        [0xFF, 0xFE, rest @ ..] => {
            let units: Vec<u16> = rest
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&units)
        }
        [0xFE, 0xFF, rest @ ..] => {
            let units: Vec<u16> = rest
                .chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&units)
        }
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_table_parses_completely() {
        let c = builtin();
        // 每一列都要變成一隻寵物。少一筆就代表解析吃掉了東西
        let data_lines = BUILTIN_TSV
            .lines()
            .filter(|l| !l.starts_with('#') && !l.is_empty())
            .count();
        assert_eq!(c.pets.len(), data_lines, "有列沒被解析");
        assert!(c.pets.len() > 700, "內建表只剩 {} 筆", c.pets.len());
    }

    /// 上游的兩種列形狀都要進得來 —— 精簡列（無編號／無種族）曾經被整批丟掉。
    #[test]
    fn builtin_keeps_both_row_shapes() {
        let c = builtin();
        let full = c.pets.iter().filter(|p| p.id.is_some()).count();
        let slim = c.pets.len() - full;
        assert!(full > 250, "有編號的只剩 {full} 筆");
        assert!(slim > 400, "無編號的精簡列只剩 {slim} 筆 —— 又被丟掉了");

        // 精簡列缺編號／種族／技能格，但檔次必須是齊的
        let slim_pet = c.find_by_name("小可愛優奈").expect("找不到精簡列的樣本");
        assert_eq!(slim_pet.id, None);
        assert_eq!(slim_pet.race, RACE_UNKNOWN);
        assert_eq!(slim_pet.skills, None);
        assert_eq!(slim_pet.grow, [28, 28, 28, 28, 28]);
        assert_eq!(slim_pet.bprate, 0.2);
    }

    #[test]
    fn builtin_entries_look_sane() {
        let c = builtin();
        let tiger = c.find_by_name("虎人").expect("找不到虎人");
        assert_eq!(tiger.id, Some(1));
        assert_eq!(tiger.race, 0);
        assert_eq!(tiger.grow, [22, 26, 17, 19, 16]);
        assert_eq!(tiger.bprate, 0.2);
        assert_eq!(tiger.skills, Some(7));

        for p in &c.pets {
            assert!(!p.name.is_empty(), "編號 {:?} 沒有名字", p.id);
            assert!(
                p.race as usize <= RACE_UNKNOWN as usize,
                "{} 的種族碼 {} 超界",
                p.name,
                p.race
            );
            assert!(
                p.bprate > 0.0 && p.bprate < 1.0,
                "{} 的倍率 {}",
                p.name,
                p.bprate
            );
            assert!(
                p.grow.iter().all(|&g| (0..=110).contains(&g)),
                "{} 檔次超界",
                p.name
            );
        }
    }

    #[test]
    fn race_tabs_cover_every_race() {
        let mut sorted = RACE_TAB_ORDER;
        sorted.sort_unstable();
        assert_eq!(sorted, [0, 1, 2, 3, 4, 5, 6, 7, 8, RACE_UNKNOWN]);
        let c = builtin();
        let total: usize = RACE_TAB_ORDER.iter().map(|&r| c.by_race(r).len()).sum();
        assert_eq!(total, c.pets.len(), "有寵物不屬於任何分頁");
    }

    /// UTF-16-LE with BOM ＋ CRLF —— 就是 `spetdata.ini` 的真實格式。
    fn utf16le(s: &str) -> Vec<u8> {
        let mut v = vec![0xFF, 0xFE];
        for u in s.encode_utf16() {
            v.extend_from_slice(&u.to_le_bytes());
        }
        v
    }

    const SAMPLE: &str = "[自製寵物]\r\n\
        編號100=破曉之刃,27 16 25 15 37 20 6 0 0 40 60 8\r\n\
        編號146=火焰牛鬼領主,14 6 12 48 45 20 8 0 0 50 50 9\r\n\
        [龍的砂時計]\r\n\
        備份日期=2026/4/26\r\n\
        [檢查資料]\r\n\
        版本=魔物觀測者 v3.12\r\n";

    #[test]
    fn parses_the_documented_ini_shape() {
        let c = parse_spetdata(&utf16le(SAMPLE)).unwrap();
        assert_eq!(c.pets.len(), 2, "只有 [自製寵物] 段的列才算寵物");

        let p = &c.pets[0];
        assert_eq!(p.id, Some(100));
        assert_eq!(p.name, "破曉之刃");
        assert_eq!(p.grow, [27, 16, 25, 15, 37]);
        assert_eq!(p.bprate, 0.2, "20 是 ×100 表示");
        assert_eq!(p.race, 6);
        assert_eq!(p.element, Some([0, 0, 40, 60]));
        assert_eq!(p.skills, Some(8));

        assert_eq!(c.pets[1].name, "火焰牛鬼領主");
        assert_eq!(c.pets[1].race, 8);
    }

    #[test]
    fn element_columns_always_total_one_hundred() {
        let c = parse_spetdata(&utf16le(SAMPLE)).unwrap();
        for p in &c.pets {
            let e = p.element.unwrap();
            assert_eq!(e.iter().sum::<i32>(), 100, "{} 的地水火風不是 100", p.name);
        }
    }

    /// 現行上游（`cg-static.tonyq.org`）的第 12 欄是 50..127，不是技能格數。
    ///
    /// 這條釘住的是「不要把那串當技能顯示」——不然圖鑑會寫「技能格 125」，
    /// 而寵物搜尋的技能下限會拿去跟一個完全不同的量比大小。
    #[test]
    fn the_new_upstream_twelfth_column_is_not_read_as_skill_slots() {
        // 檔次和 = 20+36+9+42+16 = 123，第 12 欄 123 —— 現行上游就長這樣
        let newer = "[自製寵物]\r\n\
            編號1069=小白龍,20 36 9 42 16 20 7 0 100 0 0 123\r\n";
        let c = parse_spetdata(&utf16le(newer)).unwrap();
        let p = &c.pets[0];
        assert_eq!(p.grow, [20, 36, 9, 42, 16], "檔次照讀");
        assert_eq!(p.element, Some([0, 100, 0, 0]), "屬性照讀");
        assert_eq!(p.skills, None, "123 不是技能格數，寧可不填也不要填錯");

        // 舊世代（6..10）還是照舊當技能格數讀
        assert_eq!(skill_slots(8), Some(8));
        assert_eq!(skill_slots(MAX_SKILL_SLOTS), Some(MAX_SKILL_SLOTS));
        assert_eq!(skill_slots(MAX_SKILL_SLOTS + 1), None);
        assert_eq!(skill_slots(0), None, "0 是「沒填」不是「零格」");
    }

    #[test]
    fn accepts_utf8_without_bom_too() {
        // 2spetdata.ini 是 UTF-8 無 BOM
        let c = parse_spetdata(SAMPLE.as_bytes()).unwrap();
        assert_eq!(c.pets.len(), 2);
        assert_eq!(c.pets[0].name, "破曉之刃");
    }

    /// 檔尾的 cp950 位元組不該讓整份檔案解不出來。
    #[test]
    fn survives_a_non_utf8_tail() {
        let mut bytes = "[自製寵物]\n編號1=甲,1 2 3 4 5 20 0 100 0 0 0 6\n"
            .as_bytes()
            .to_vec();
        bytes.extend_from_slice(&[0xB5, 0x4C, 0xA5, 0xBB, 0x0A]); // cp950 亂碼尾
        let c = parse_spetdata(&bytes).unwrap();
        assert_eq!(c.pets.len(), 1);
        assert_eq!(c.pets[0].grow, [1, 2, 3, 4, 5]);
    }

    #[test]
    fn later_records_win_on_duplicate_id() {
        let src = "[自製寵物]\n\
            編號7=舊的,1 1 1 1 1 20 0 100 0 0 0 6\n\
            編號7=新的,2 2 2 2 2 20 0 100 0 0 0 6\n";
        let c = parse_spetdata(src.as_bytes()).unwrap();
        assert_eq!(c.pets.len(), 1);
        assert_eq!(c.pets[0].name, "新的");
        assert_eq!(c.pets[0].grow, [2, 2, 2, 2, 2]);
    }

    #[test]
    fn rejects_a_file_with_no_pet_section() {
        let err = parse_spetdata("[檢查資料]\n版本=x\n".as_bytes()).unwrap_err();
        assert!(matches!(err, ParseError::NoPets { .. }));
    }

    #[test]
    fn skips_malformed_records_without_losing_good_ones() {
        let src = "[自製寵物]\n\
            編號1=太少欄,1 2 3\n\
            編號=沒有編號,1 2 3 4 5 20 0 100 0 0 0 6\n\
            編號2=好的,9 8 7 6 5 20 3 0 100 0 0 7\n";
        let c = parse_spetdata(src.as_bytes()).unwrap();
        assert_eq!(c.pets.len(), 1);
        assert_eq!(c.pets[0].name, "好的");
        assert_eq!(c.pets[0].race, 3);
    }

    // ── 自製寵物 ────────────────────────────────────────────────────────────

    fn custom_pet(name: &str, grow: [i32; AXES]) -> Pet {
        Pet {
            id: None,
            name: name.into(),
            race: 0,
            grow,
            bprate: 0.2,
            skills: None,
            element: None,
            custom: true,
        }
    }

    #[test]
    fn overlay_appends_a_new_name() {
        let mut c = builtin();
        let before = c.pets.len();
        c.overlay(&[custom_pet("我抓的怪", [30, 30, 30, 30, 30])]);
        assert_eq!(c.pets.len(), before + 1);
        let p = c.find_by_name("我抓的怪").expect("自製寵物沒進圖鑑");
        assert!(p.custom);
        assert_eq!(p.grow, [30, 30, 30, 30, 30]);
    }

    /// 同名＝覆寫，不是多一筆。改過的內建寵物在清單上只能有一隻。
    #[test]
    fn overlay_replaces_a_base_pet_in_place() {
        let mut c = builtin();
        let before = c.pets.len();
        let at = c.pets.iter().position(|p| p.name == "虎人").unwrap();

        c.overlay(&[custom_pet("虎人", [99, 1, 1, 1, 1])]);

        assert_eq!(c.pets.len(), before, "覆寫不該讓圖鑑變長");
        assert_eq!(c.pets.iter().filter(|p| p.name == "虎人").count(), 1);
        assert_eq!(c.pets[at].grow, [99, 1, 1, 1, 1], "應該就地換掉，順序不變");
        assert!(c.pets[at].custom);
    }

    /// 覆寫只動記憶體裡這一份 —— 內建表是編進二進位檔的常數，重新取一份就回來了。
    #[test]
    fn overlay_never_touches_the_builtin_table() {
        let mut c = builtin();
        c.overlay(&[custom_pet("虎人", [99, 1, 1, 1, 1])]);
        assert_eq!(
            builtin().find_by_name("虎人").unwrap().grow,
            [22, 26, 17, 19, 16]
        );
    }

    /// 疊進來的一律標成自製 —— 不然刪除鈕會判斷錯，變成刪不掉自己加的東西。
    #[test]
    fn overlay_marks_everything_custom() {
        let mut c = builtin();
        let mut p = custom_pet("旗標沒設好", [1, 1, 1, 1, 1]);
        p.custom = false;
        c.overlay(&[p]);
        assert!(c.find_by_name("旗標沒設好").unwrap().custom);
    }

    #[test]
    fn base_catalogs_are_never_custom() {
        assert!(builtin().pets.iter().all(|p| !p.custom));
        let c = parse_spetdata(&utf16le(SAMPLE)).unwrap();
        assert!(
            c.pets.iter().all(|p| !p.custom),
            "spetdata.ini 是上游資料，不是自製的"
        );
    }

    #[test]
    fn validation_accepts_a_plain_pet() {
        assert_eq!(custom_pet("正常", [30, 30, 30, 30, 30]).validate(), Ok(()));
    }

    #[test]
    fn validation_rejects_the_things_that_break_the_engine() {
        let bad = |f: fn(&mut Pet)| {
            let mut p = custom_pet("測試", [30, 30, 30, 30, 30]);
            f(&mut p);
            p.validate().unwrap_err()
        };

        assert_eq!(bad(|p| p.name = "   ".into()), PetError::EmptyName);
        assert_eq!(
            bad(|p| p.grow[3] = MAX_TIER + 1),
            PetError::TierOutOfRange {
                axis: 3,
                value: MAX_TIER + 1
            }
        );
        assert_eq!(
            bad(|p| p.grow[0] = -1),
            PetError::TierOutOfRange { axis: 0, value: -1 }
        );
        assert_eq!(
            bad(|p| p.race = RACE_UNKNOWN + 1),
            PetError::BadRace(RACE_UNKNOWN + 1)
        );
        assert_eq!(bad(|p| p.bprate = 0.0), PetError::BadRate(0.0));
        assert_eq!(bad(|p| p.bprate = -0.2), PetError::BadRate(-0.2));
        // NaN 的比較全是 false，很容易從驗證裡溜掉，然後把整條推算變成 NaN
        assert!(matches!(bad(|p| p.bprate = f64::NAN), PetError::BadRate(v) if v.is_nan()));
        assert_eq!(
            bad(|p| p.element = Some([10, 10, 10, 10])),
            PetError::ElementNotHundred(40)
        );
    }

    /// 邊界值本身是合法的 —— 檔次 0 與 110 都在表裡。
    #[test]
    fn validation_allows_the_boundaries() {
        assert!(custom_pet("下界", [0; AXES]).validate().is_ok());
        assert!(custom_pet("上界", [MAX_TIER; AXES]).validate().is_ok());

        let mut p = custom_pet("四屬性", [1; AXES]);
        p.element = Some([25, 25, 25, 25]);
        assert!(p.validate().is_ok());
        p.element = None;
        assert!(p.validate().is_ok(), "沒填地水火風不該被擋");
    }

    #[test]
    fn search_and_lookup() {
        let c = builtin();
        assert!(c.search("蝙蝠").len() > 5);
        assert!(c.search("").is_empty());
        assert!(c.search("這隻不存在").is_empty());
        assert_eq!(c.find_by_id(1).map(|p| p.name.as_str()), Some("虎人"));
    }

    // ── 種族改名 ────────────────────────────────────────────────────────────

    /// 種族**碼**的意義沒變，變的只有顯示名稱。
    ///
    /// 對照關係是拿 originmood 的 1069 筆逐名 join 現行 `spetdata.ini` 得到的：
    /// 九個種族全部一對一、零筆衝突（例如碼 2 的 178 隻在新表全都是「飛行系」，
    /// 沒有半隻落到別的種族去）。這裡把那張表釘住 —— 改任何一格都要能說出
    /// 是哪次 join 得到的新結論。
    #[test]
    fn the_race_codes_still_mean_what_the_original_meant() {
        // (碼, 原程式的名字, 現用的名字)
        let table = [
            (0u8, "野獸系", "野獸系"),
            (1, "死靈系", "不死系"),
            (2, "邪魔系", "飛行系"),
            (3, "蟲系", "昆蟲系"),
            (4, "植物系", "植物系"),
            (5, "能源系", "特殊系"),
            (6, "各種系", "金屬系"),
            (7, "龍族系", "龍系"),
            (8, "人族系", "人形系"),
        ];
        for (code, was, now) in table {
            assert_eq!(race_name(code), now, "碼 {code}（原程式的「{was}」）");
            assert_eq!(race_code(now), code, "「{now}」反查不回碼 {code}");
        }
        assert_eq!(table.len(), MAX_RACE as usize + 1, "遊戲種族剛好 0..=MAX_RACE");

        // 「特殊系」現在是碼 5。原程式的碼 9 也叫特殊系，兩個不能同時存在 ——
        // 碼 9 已退休，所以這個名字只會指到一個地方。
        assert_eq!(race_code("特殊系"), 5);
        assert_eq!(race_name(RACE_UNKNOWN), "其他");
        assert_eq!(RACE_NAMES.len(), RACE_UNKNOWN as usize + 1);
    }

    /// 退休的碼 9 不能還躺在內建表裡 —— 不然那兩隻會顯示成「其他」以外的東西。
    #[test]
    fn the_retired_race_code_is_gone_from_the_builtin_table() {
        let c = builtin();
        assert!(
            c.pets.iter().all(|p| p.race <= MAX_RACE || p.race == RACE_UNKNOWN),
            "內建表還有種族碼在 MAX_RACE 與 RACE_UNKNOWN 之間"
        );
        // 那兩隻公雞併進「其他」了
        for name in ["大公雞(缺)", "死神公雞"] {
            let p = c.find_by_name(name).unwrap_or_else(|| panic!("{name} 不見了"));
            assert_eq!(p.race, RACE_UNKNOWN, "{name}");
        }
    }

    #[test]
    fn unknown_race_names_fall_back_to_other() {
        assert_eq!(race_code("能源系"), RACE_UNKNOWN, "舊名字不該還認得");
        assert_eq!(race_code(""), RACE_UNKNOWN);
        assert_eq!(race_code("  龍系  "), 7, "前後空白要吃掉");
    }

    // ── originmood CSV ──────────────────────────────────────────────────────

    const CSV: &str = "名稱,種族,體力,力量,防禦,速度,魔法,技格,總檔,屬性,技能,image,任務用途
        奧菲兒,人形系,43,6,6,20,50,9,125,地10,超強石化魔法LV10,/img/a.gif,
        水雷猴,野獸系,26,19,21,18,39,8,123,地1水9,混亂衝擊波LV4,/img/b.gif,
        太晶龍蝦霸王,昆蟲系,35,50,14,20,6,8,125,火9風1,氣功彈LV2,/img/c.gif,
";

    #[test]
    fn originmood_csv_parses() {
        let c = parse_originmood(CSV).expect("解不出來");
        assert_eq!(c.pets.len(), 3);

        let p = &c.pets[0];
        assert_eq!(p.name, "奧菲兒");
        assert_eq!(p.race, 8, "人形系");
        // 欄位順序是 體力 力量 防禦 速度 魔法 —— 對上 [HP, ATK, DEF, AGI, MP]
        assert_eq!(p.grow, [43, 6, 6, 20, 50]);
        assert_eq!(p.bprate, DEFAULT_BPRATE);
        assert_eq!(p.skills, Some(9), "技格 9 才是技能格數（總檔 125 不是）");
        assert_eq!(p.element, Some([100, 0, 0, 0]), "地10");
        assert!(!p.custom, "上游資料不是自製");
        assert_eq!(p.id, None, "這份沒有編號欄");

        assert_eq!(c.pets[1].element, Some([10, 90, 0, 0]), "地1水9");
        assert_eq!(c.pets[2].element, Some([0, 0, 90, 10]), "火9風1");

        for p in &c.pets {
            p.validate().unwrap_or_else(|e| panic!("{}: {e}", p.name));
        }
    }

    /// 欄位**照表頭找**，所以上游在中間插一欄不會讓資料錯位。
    #[test]
    fn originmood_columns_are_found_by_header_not_position() {
        let moved = "種族,名稱,魔法,速度,防禦,力量,體力,屬性,技格,總檔
            人形系,奧菲兒,50,20,6,6,43,地10,9,125
";
        let c = parse_originmood(moved).expect("解不出來");
        assert_eq!(c.pets[0].name, "奧菲兒");
        assert_eq!(c.pets[0].grow, [43, 6, 6, 20, 50]);
        assert_eq!(c.pets[0].skills, Some(9));
    }

    /// 上游那筆髒屬性：`水10(水5火5)` 加起來是 20。
    ///
    /// 只取括號前面那段（＝100%），整串加總會得到 200 而被 `validate` 擋掉
    /// —— 那隻就會整個消失，而消失比顯示上游的髒資料糟。
    #[test]
    fn the_dirty_element_row_survives() {
        let csv = "名稱,種族,體力,力量,防禦,速度,魔法,技格,總檔,屬性
            暴雪魔獸,野獸系,30,30,30,30,30,8,150,水10(水5火5)
";
        let c = parse_originmood(csv).expect("解不出來");
        assert_eq!(c.pets.len(), 1, "髒資料不該讓整隻消失");
        assert_eq!(c.pets[0].element, Some([0, 100, 0, 0]));
        c.pets[0].validate().expect("括號前那段本身是乾淨的");
    }

    #[test]
    fn a_csv_without_the_columns_we_need_is_an_error() {
        assert!(parse_originmood("").is_err());
        assert!(parse_originmood("名稱,種族
奧菲兒,人形系
").is_err(), "少了檔次欄");
    }

    // ── 兩份上游揉在一起 ────────────────────────────────────────────────────

    #[test]
    fn merge_upstream_is_a_union_with_the_newer_side_winning() {
        let mut base = Catalog {
            source: "舊".into(),
            pets: vec![
                Pet { id: Some(7), name: "同名".into(), race: 0, grow: [1, 1, 1, 1, 1],
                      bprate: 0.2, skills: None, element: None, custom: false },
                Pet { id: Some(8), name: "只有舊的有".into(), race: 0, grow: [2, 2, 2, 2, 2],
                      bprate: 0.2, skills: None, element: None, custom: false },
            ],
        };
        let newer = vec![
            Pet { id: None, name: "同名".into(), race: 3, grow: [9, 9, 9, 9, 9],
                  bprate: 0.2, skills: Some(8), element: None, custom: false },
            Pet { id: None, name: "只有新的有".into(), race: 4, grow: [5, 5, 5, 5, 5],
                  bprate: 0.2, skills: None, element: None, custom: false },
        ];
        base.merge_upstream(&newer);

        assert_eq!(base.pets.len(), 3, "聯集：舊的沒被丟掉、新的接在後面");
        let hit = base.find_by_name("同名").unwrap();
        assert_eq!(hit.grow, [9, 9, 9, 9, 9], "同名以新的為準");
        assert_eq!(hit.race, 3);
        assert_eq!(hit.id, Some(7), "新的沒編號時要留著舊的");
        assert!(!hit.custom, "兩份上游揉一揉還是上游");
        assert!(base.find_by_name("只有舊的有").is_some());
        assert!(base.find_by_name("只有新的有").is_some());
    }

    /// 揉完之後自製層還是疊得上去，而且蓋的是揉出來的那份。
    #[test]
    fn custom_pets_still_overlay_the_merged_table() {
        let mut c = builtin();
        c.merge_upstream(&parse_originmood(CSV).unwrap().pets);
        let before = c.pets.len();

        c.overlay(&[Pet {
            id: None, name: "奧菲兒".into(), race: 0, grow: [1, 1, 1, 1, 1],
            bprate: 0.2, skills: None, element: None, custom: false,
        }]);
        assert_eq!(c.pets.len(), before, "同名覆寫不該多一筆");
        let p = c.find_by_name("奧菲兒").unwrap();
        assert_eq!(p.grow, [1, 1, 1, 1, 1]);
        assert!(p.custom, "自製層一定要標成可編輯");
    }
}
