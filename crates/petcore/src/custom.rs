//! 自製寵物的存檔。
//!
//! # 為什麼不寫回 `spetdata.ini`
//!
//! 原程式是把自製寵物寫進 `spetdata.ini` 的 `[自製寵物]` 段
//! （走 `WritePrivateProfileStringW`，所以是 UTF-16-LE）。
//! 移植版**不這樣做**，理由如下：
//!
//! 1. 寫檔一律 UTF-8，而 `spetdata.ini` 的既定格式是 UTF-16-LE ——
//!    照寫下去，原程式就讀不回來了。
//! 2. `spetdata.ini` 是**自動更新**下來的（`魔物觀測者_自動更新檔次.bat` 從
//!    `cg-static.tonyq.org` 抓整份蓋掉）。寫在那裡的東西下次更新就沒了。
//!
//! 所以自製寵物存在**自己的地方**，UTF-8 JSON，載入時疊到圖鑑上
//! （`Catalog::overlay`）。上游那份始終唯讀。
//!
//! # 存在哪
//!
//! 網頁版沒有檔案系統，實際落點是瀏覽器的 `localStorage`（見 [`Store`]）。
//! **格式一個位元組都沒變** —— 同一份 JSON，只是換了容器，所以桌面版存下來的
//! `custom_pets.json` 可以直接貼進去，反過來也行。
//!
//! # 格式
//!
//! ```json
//! { "version": 1, "pets": [ { "name": "我抓的怪", "race": 0, "grow": [30,30,30,30,30] } ] }
//! ```
//!
//! 沒填的欄位就不寫出來 —— 編號尤其如此：那是動畫資源的鍵（`C{N}`），
//! 自己加的寵物本來就沒有對應動畫，不該編一個假的（`CLAUDE.md` §2.2）。
//! 原程式的編輯畫面也沒有編號欄位，它是存檔時自己配的。

use serde::{Deserialize, Serialize};

use petdata::{Pet, AXES, RACE_UNKNOWN};

/// 存檔的鍵／檔名。桌面版是檔名，網頁版是 `localStorage` 的鍵。
pub const FILE_NAME: &str = "custom_pets.json";

/// 檔案格式版本。之後若要改結構，靠這個判斷要不要轉檔。
const VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct File {
    version: u32,
    pets: Vec<PetRow>,
}

/// 檔案裡的一隻寵物。
///
/// 刻意跟 [`Pet`] 分開：`Pet` 有 `custom` 旗標與 `race_name` 之類的衍生資訊，
/// 那些是執行期的東西，不該落到檔案裡。這裡只存使用者真正輸入過的欄位。
///
/// 這份格式與桌面版的 `custom_pets.json` 逐位元相同（`the_desktop_file_format_still_loads`
/// 釘住這件事），所以兩邊的自製寵物可以互相搬。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PetRow {
    name: String,
    race: u8,
    /// `[體力, 力量, 強度, 速度, 魔法]`
    grow: [i32; AXES],
    #[serde(default = "default_bprate")]
    bprate: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    skills: Option<i32>,
    /// 地水火風，總和 100。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    element: Option<[i32; 4]>,
}

fn default_bprate() -> f64 {
    0.2
}

impl From<&Pet> for PetRow {
    fn from(p: &Pet) -> Self {
        Self {
            name: p.name.clone(),
            race: p.race,
            grow: p.grow,
            bprate: p.bprate,
            id: p.id,
            skills: p.skills,
            element: p.element,
        }
    }
}

impl From<PetRow> for Pet {
    fn from(s: PetRow) -> Self {
        Pet {
            id: s.id,
            name: s.name,
            race: s.race.min(RACE_UNKNOWN),
            grow: s.grow,
            bprate: s.bprate,
            skills: s.skills,
            element: s.element,
            custom: true,
        }
    }
}

/// 自製寵物放哪 —— 由平台實作。
///
/// 抽成 trait 是為了讓 [`crate::Engine`] 完全不知道自己跑在瀏覽器還是檔案系統上。
/// 存的內容是同一串 JSON，差別只在誰負責把它放好。
pub trait Store {
    /// 讀出整串 JSON。`None` 代表**還沒存過** —— 那不是錯誤，第一次跑本來就沒有。
    fn read(&self) -> Result<Option<String>, String>;
    /// 覆寫。
    fn write(&self, text: &str) -> Result<(), String>;
    /// 顯示給使用者看的位置（寵物檔案視窗會秀）。
    fn location(&self) -> String;
}

/// 把 JSON 解成寵物清單。
///
/// 解不開就回錯，**不要默默當成空的** —— 那代表使用者的資料還在、只是這支
/// 程式看不懂，這種時候要吵，不然下一次存檔就把它蓋掉了。
pub fn decode(text: &str) -> Result<Vec<Pet>, String> {
    let file: File = serde_json::from_str(text)
        .map_err(|e| format!("自製寵物的資料格式不對：{e}（原本那份沒有動，可以手動修）"))?;
    if file.version > VERSION {
        return Err(format!(
            "這是版本 {} 的存檔，這支程式只認到版本 {VERSION}",
            file.version
        ));
    }
    Ok(file.pets.into_iter().map(Pet::from).collect())
}

/// 轉成 JSON。UTF-8，無 BOM（Rust 的 `String` 本來就不會有）。
pub fn encode(pets: &[Pet]) -> Result<String, String> {
    let file = File {
        version: VERSION,
        pets: pets.iter().map(PetRow::from).collect(),
    };
    serde_json::to_string_pretty(&file).map_err(|e| format!("轉不成 JSON：{e}"))
}

/// 從 [`Store`] 讀。沒存過就是空清單。
pub fn load(store: &dyn Store) -> Result<Vec<Pet>, String> {
    match store.read()? {
        Some(text) => decode(&text),
        None => Ok(Vec::new()),
    }
}

/// 寫進 [`Store`]。
pub fn save(store: &dyn Store, pets: &[Pet]) -> Result<(), String> {
    store.write(&encode(pets)?)
}

/// 完全不存 —— 測試與「無痕模式擋掉 localStorage」時的退路。
///
/// 讀永遠是空的、寫永遠成功。這樣使用者在無痕視窗裡還是能加寵物來試算，
/// 只是關掉就沒了；比整個功能壞掉好。
#[derive(Debug, Default, Clone, Copy)]
pub struct NullStore;

impl Store for NullStore {
    fn read(&self) -> Result<Option<String>, String> {
        Ok(None)
    }
    fn write(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn location(&self) -> String {
        "（這次不會存起來）".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// 記憶體版的 [`Store`]，行為跟 localStorage 一樣。
    #[derive(Default)]
    pub struct MemStore(RefCell<Option<String>>);

    impl Store for MemStore {
        fn read(&self) -> Result<Option<String>, String> {
            Ok(self.0.borrow().clone())
        }
        fn write(&self, text: &str) -> Result<(), String> {
            *self.0.borrow_mut() = Some(text.into());
            Ok(())
        }
        fn location(&self) -> String {
            "(memory)".into()
        }
    }

    fn pet(name: &str) -> Pet {
        Pet {
            id: None,
            name: name.into(),
            race: 2,
            grow: [30, 25, 18, 20, 22],
            bprate: 0.2,
            skills: None,
            element: None,
            custom: true,
        }
    }

    #[test]
    fn nothing_stored_is_an_empty_list_not_an_error() {
        assert_eq!(load(&MemStore::default()), Ok(Vec::new()));
    }

    #[test]
    fn round_trips_through_the_store() {
        let store = MemStore::default();
        let mut p = pet("我抓的怪");
        p.id = Some(9001);
        p.skills = Some(7);
        p.element = Some([10, 20, 30, 40]);

        save(&store, std::slice::from_ref(&p)).unwrap();
        assert_eq!(load(&store).unwrap(), vec![p]);
    }

    /// 一律 UTF-8 且中文是原樣的字，不是 `\uXXXX` 跳脫序列。
    #[test]
    fn the_text_is_utf8_without_a_bom() {
        let json = encode(&[pet("破曉之刃")]).unwrap();
        assert!(!json.starts_with('\u{feff}'), "不能有 BOM");
        assert!(json.contains("破曉之刃"), "中文被跳脫了：{json}");
    }

    /// 讀回來的一定是自製 —— 存檔裡本來就沒有 `custom` 這一欄。
    #[test]
    fn loaded_pets_are_always_custom() {
        let store = MemStore::default();
        save(&store, &[pet("甲")]).unwrap();
        assert!(load(&store).unwrap().iter().all(|p| p.custom));
    }

    /// 只有使用者填過的欄位會被寫出來。
    #[test]
    fn absent_fields_are_left_out() {
        let json = encode(&[pet("沒有編號")]).unwrap();
        assert!(!json.contains("\"id\""), "沒有編號就不該寫 id 欄：{json}");
        assert!(!json.contains("\"element\""));
        assert!(json.contains("\"bprate\""), "倍率一定有值");
    }

    #[test]
    fn corrupt_data_reports_instead_of_silently_starting_over() {
        let err = decode("{ 這不是 JSON").unwrap_err();
        assert!(err.contains("格式不對"), "{err}");
    }

    #[test]
    fn a_newer_version_is_refused() {
        assert!(decode(r#"{"version":999,"pets":[]}"#)
            .unwrap_err()
            .contains("999"));
    }

    /// 存空清單是合法的 —— 使用者可以把自製寵物全刪掉。
    #[test]
    fn an_empty_list_round_trips() {
        let store = MemStore::default();
        save(&store, &[]).unwrap();
        assert_eq!(load(&store), Ok(Vec::new()));
    }

    /// 桌面版存下來的檔案要讀得回來 —— 換容器不換格式，這條就是保證。
    #[test]
    fn the_desktop_file_format_still_loads() {
        let on_disk = r#"{
          "version": 1,
          "pets": [
            { "name": "我抓的怪", "race": 0, "grow": [30,30,30,30,30], "bprate": 0.2 }
          ]
        }"#;
        let pets = decode(on_disk).unwrap();
        assert_eq!(pets.len(), 1);
        assert_eq!(pets[0].name, "我抓的怪");
        assert_eq!(pets[0].grow, [30, 30, 30, 30, 30]);
    }

    /// 寫失敗要傳上去，不能吞掉。
    #[test]
    fn a_failing_store_surfaces_the_error() {
        struct Broken;
        impl Store for Broken {
            fn read(&self) -> Result<Option<String>, String> {
                Ok(None)
            }
            fn write(&self, _: &str) -> Result<(), String> {
                Err("空間滿了".into())
            }
            fn location(&self) -> String {
                "(broken)".into()
            }
        }
        assert_eq!(save(&Broken, &[pet("甲")]), Err("空間滿了".into()));
    }
}
