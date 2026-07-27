//! 「噬生・魔物觀測者」remake 的應用層。
//!
//! 這一層做三件事：持有目前的寵物圖鑑、把前端的 JSON 轉成 [`petcalc`] 的型別、
//! 把結果轉回去。真正的計算全在 `petcalc` 裡，所以那個 crate 可以獨立測試、
//! 也不需要 serde。
//!
//! # 為什麼不直接寫在 wasm 綁定裡
//!
//! [`Engine::dispatch`] 收的是「指令名 ＋ JSON 參數」，回的也是 JSON ——
//! 跟 Tauri 的 `invoke(cmd, args)` 同一個形狀。這麼做有兩個好處：
//!
//! * 整個指令介面可以用普通的 `cargo test` 測，不必開瀏覽器
//! * `petweb` 只剩一層薄殼，沒有邏輯可以走鐘
//!
//! 平台差異只有一個 —— 自製寵物存哪（[`custom::Store`]）。

pub mod custom;
pub mod dto;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use custom::Store;
use dto::{
    build_guess_resp, forward_row, CatalogDto, Constants, ForwardReq, ForwardRow,
    GuessReq, GuessResp, PetDto, ProbabilityReq, ProbabilityResp, RefineReq, RefineResp, SearchReq,
    SearchResp, SeriesReq,
};
use petcalc::{Candidate, Distribution, GrowRange, MatchMode};
use petdata::{Catalog, Pet};

/// 回傳給前端的解上限。列舉出來的解可能上萬筆，全部丟過去只會塞爆表格；
/// 統計（機率分布）仍然是用**全部**的解算的，只有列表被截斷。
pub const MAX_RESULT_ROWS: usize = 200;

/// 模擬表格一次最多算幾級。
pub const MAX_SERIES_ROWS: i32 = 300;

/// 遊戲的等級上限。
pub const MAX_LEVEL: i32 = 255;

// ── 圖鑑 ────────────────────────────────────────────────────────────────────

/// 圖鑑的幾份資料：內建的、線上的、使用者自己的、合起來的。
///
/// 擺在同一個結構裡是刻意的 —— `merged` 是其他幾個算出來的，分開放就會有
/// 「改了 upstream 但 merged 還是舊的」這種中間狀態被別的指令讀到。
///
/// 疊法是 **內建表 → 線上圖鑑 → 自製**，每一層同名的蓋掉上一層、新的接在後面：
///
/// | 層 | 來源 | 同名時 | 可編輯 |
/// |---|---|---|---|
/// | 內建表 | 編進 wasm 的 TSV | 墊底 | ✗ |
/// | 線上圖鑑 | originmood 的 CSV，執行期抓 | **蓋掉內建的** | ✗ |
/// | 自製 | `localStorage` | 蓋掉上面兩層 | ✓ |
///
/// 線上那層抓不到就是 `None`，那時只剩內建表 —— 少了 587 隻新寵，
/// 但程式照樣能用。網路不通不該讓整個計算機打不開。
struct Library {
    /// 內建表。編進二進位檔的底，永遠在。
    builtin: Catalog,
    /// 線上抓下來的上游圖鑑。**唯讀**。
    upstream: Option<Catalog>,
    /// 使用者自己加／改的寵物。
    custom: Vec<Pet>,
    /// 三層疊完的結果 —— 其他指令看到的就是這一份。
    merged: Catalog,
}

impl Library {
    fn new(builtin: Catalog, custom: Vec<Pet>) -> Self {
        let mut lib = Self {
            builtin,
            upstream: None,
            custom,
            merged: Catalog::default(),
        };
        lib.rebuild();
        lib
    }

    fn rebuild(&mut self) {
        self.merged = self.builtin.clone();
        if let Some(up) = &self.upstream {
            self.merged.merge_upstream(&up.pets);
            self.merged.source = format!("{} ＋ {}", self.builtin.source, up.source);
        }
        self.merged.overlay(&self.custom);
    }

    /// 換上線上圖鑑（`None` ＝ 只用內建表）。自製的那層原封不動疊回去。
    fn set_upstream(&mut self, upstream: Option<Catalog>) {
        self.upstream = upstream;
        self.rebuild();
    }

    /// 新增或覆寫一隻自製寵物，然後存起來。
    ///
    /// `rename_from` 是改名前的舊名字：名字就是鍵，改名等於換一筆，
    /// 舊的那筆得一起收掉，不然清單上會同時出現新舊兩隻。
    fn put(
        &mut self,
        pet: Pet,
        rename_from: Option<&str>,
        store: &dyn Store,
    ) -> Result<(), String> {
        pet.validate().map_err(|e| e.to_string())?;
        if let Some(old) = rename_from.filter(|o| *o != pet.name) {
            self.custom.retain(|p| p.name != old);
        }
        match self.custom.iter().position(|p| p.name == pet.name) {
            Some(i) => self.custom[i] = pet,
            None => self.custom.push(pet),
        }
        self.commit(store)
    }

    /// 移除一隻自製寵物。蓋掉上游那筆的話，上游的就會露出來。
    fn remove(&mut self, name: &str, store: &dyn Store) -> Result<(), String> {
        let before = self.custom.len();
        self.custom.retain(|p| p.name != name);
        if self.custom.len() == before {
            return Err(format!("「{name}」不是自製寵物，刪不了"));
        }
        self.commit(store)
    }

    /// 存起來，成功才更新記憶體裡的合併結果。
    ///
    /// 存失敗時要把 `custom` 回捲：不然畫面上看起來改好了，重開卻不見了。
    fn commit(&mut self, store: &dyn Store) -> Result<(), String> {
        match custom::save(store, &self.custom) {
            Ok(()) => {
                self.rebuild();
                Ok(())
            }
            Err(e) => {
                self.custom = custom::load(store).unwrap_or_default();
                self.rebuild();
                Err(e)
            }
        }
    }
}

struct GuessCache {
    candidates: Vec<Candidate>,
    dist: Distribution,
    /// 再篩選時要把結果重建成 [`GuessResp`]，那幾個欄位得跟原本那次一致。
    catalog: GrowRange,
    mode: MatchMode,
    point: i32,
}

// ── 引擎 ────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct Bootstrap {
    catalog: CatalogDto,
    constants: Constants,
    /// 自製寵物存在哪，顯示在寵物檔案視窗上。
    custom_path: String,
    /// 載入自製寵物時出的錯（資料壞掉之類）。`None` 代表沒事。
    ///
    /// 這種錯不該讓程式起不來，但也不能吞掉 —— 使用者得知道自己的資料還在，
    /// 只是這次沒讀進來，別急著再存一次把它蓋掉。
    custom_error: Option<String>,
}

pub struct Engine {
    lib: Library,
    store: Box<dyn Store>,
    /// 最後一次推算的**完整**候選與分布。
    ///
    /// 機率查詢非留這份不可：回給前端的 `candidates` 被截成
    /// [`MAX_RESULT_ROWS`] 筆，拿那些去加權會少算。
    last_guess: Option<GuessCache>,
    /// 啟動時讀自製寵物出的錯，留著給 `bootstrap` 回報。
    custom_error: Option<String>,
}

impl Engine {
    /// 建立引擎。圖鑑先用內建表，自製寵物從 `store` 讀。
    ///
    /// 讀不出來**不會**讓建立失敗 —— 先當成沒有自製寵物，錯誤留到 `bootstrap`
    /// 回報給前端。整個程式因為一份壞掉的存檔而起不來是最糟的結果。
    pub fn new(store: Box<dyn Store>) -> Self {
        let (mine, custom_error) = match custom::load(store.as_ref()) {
            Ok(pets) => (pets, None),
            Err(e) => (Vec::new(), Some(e)),
        };
        Self {
            lib: Library::new(petdata::builtin(), mine),
            store,
            last_guess: None,
            custom_error: custom_error.clone(),
        }
    }

    /// 前端啟動時的單次往返：圖鑑 ＋ 常數 ＋ 可載入的資料檔。
    pub fn bootstrap(&self) -> Bootstrap {
        Bootstrap {
            catalog: (&self.lib.merged).into(),
            constants: Constants::get(),
            custom_path: self.store.location(),
            custom_error: self.custom_error.clone(),
        }
    }

    pub fn catalog(&self) -> CatalogDto {
        (&self.lib.merged).into()
    }

    /// 疊上線上抓來的上游圖鑑（originmood 的 CSV）。自製那層會疊回去。
    ///
    /// 收的是 **CSV 原文**，解析在這裡做 —— 前端只負責把位元組搬過來，
    /// 格式的認知只有一份，就在 `petdata::parse_originmood` 裡。
    pub fn load_catalog(&mut self, csv: &str) -> Result<CatalogDto, String> {
        let cat = petdata::parse_originmood(csv).map_err(|e| format!("圖鑑解析失敗：{e}"))?;
        self.lib.set_upstream(Some(cat));
        Ok(self.catalog())
    }

    /// 拿掉線上那層，只留內建表。
    pub fn reset_catalog(&mut self) -> CatalogDto {
        self.lib.set_upstream(None);
        self.catalog()
    }

    pub fn search_pets(&self, keyword: &str) -> Vec<PetDto> {
        self.lib
            .merged
            .search(keyword)
            .into_iter()
            .map(PetDto::from)
            .collect()
    }

    /// 新增或修改一隻自製寵物（原程式的「添加寵物」／「修改」→「保存」）。
    ///
    /// 改的若是上游圖鑑裡的寵物，會變成一筆同名的自製記錄蓋在上面；
    /// 把那筆刪掉，上游的就回來了。上游那份本身完全不會被寫到。
    pub fn save_pet(
        &mut self,
        pet: PetDto,
        rename_from: Option<&str>,
    ) -> Result<CatalogDto, String> {
        self.lib.put(pet.into(), rename_from, self.store.as_ref())?;
        Ok(self.catalog())
    }

    /// 刪掉一隻自製寵物（原程式的「刪除」）。上游圖鑑裡的刪不了。
    pub fn delete_pet(&mut self, name: &str) -> Result<CatalogDto, String> {
        self.lib.remove(name, self.store.as_ref())?;
        Ok(self.catalog())
    }

    /// 單一等級的正向計算：檔次 ＋ 隨機檔 ＋ 加點 → 能力值。
    pub fn forward(&self, req: &ForwardReq) -> Result<ForwardRow, String> {
        check_level(req.lvl)?;
        Ok(forward_row(req, req.lvl))
    }

    /// 一段等級區間的正向計算 —— 「模擬」分頁的成長表。
    pub fn series(&self, req: &SeriesReq) -> Result<Vec<ForwardRow>, String> {
        check_level(req.from)?;
        check_level(req.to)?;
        if req.to < req.from {
            return Err(format!("等級區間反了：{} → {}", req.from, req.to));
        }
        if req.to - req.from + 1 > MAX_SERIES_ROWS {
            return Err(format!("等級區間太大，一次最多 {MAX_SERIES_ROWS} 級"));
        }
        Ok((req.from..=req.to)
            .map(|lvl| forward_row(&req.base, lvl))
            .collect())
    }

    /// 推算：由觀測到的能力反推檔次／加點／隨機檔，並附上機率分布。
    ///
    /// 最壞情況是 3125 組檔次 × 加點 × 1001 組隨機檔，會跑好幾秒。
    /// 網頁版靠 Web Worker 讓它不擋住畫面 —— 這裡不必知道那件事。
    pub fn guess(&mut self, req: &GuessReq) -> Result<GuessResp, String> {
        check_level(req.target.lvl)?;
        req.check_catch()?;
        req.check_ranges()?;
        let catalog: GrowRange = req.grow.into();
        let outcome = petcalc::solve(catalog, req.target.into(), req.into());
        let (resp, dist) = build_guess_resp(catalog, &outcome, MAX_RESULT_ROWS);

        // 留給 probability() 與 refine()。無解時清掉，免得查到上一次的結果。
        self.last_guess = dist.map(|dist| GuessCache {
            candidates: outcome.candidates,
            dist,
            catalog,
            mode: outcome.mode,
            point: outcome.point,
        });
        Ok(resp)
    }

    /// 推算後的再篩選 —— 原程式的「輸入更多資訊」。
    ///
    /// **不重算**：套在最後一次 [`guess`](Self::guess) 留下來的完整候選上，
    /// 這正是原程式的作法（它同樣只掃已經建好的候選表）。所以補資訊是即時的，
    /// 不必回頭把幾千組候選再列舉一次。
    pub fn refine(&self, req: &RefineReq) -> Result<RefineResp, String> {
        let cache = self
            .last_guess
            .as_ref()
            .ok_or("還沒有推算結果可以篩，先按「計算」")?;
        Ok(req.eval(
            cache.catalog,
            &cache.candidates,
            &cache.dist,
            cache.mode,
            cache.point,
            MAX_RESULT_ROWS,
        ))
    }

    /// 機率查詢 —— 原程式側欄的機率統計。
    ///
    /// 套用在**最後一次 [`guess`](Self::guess) 的完整候選**上，所以要先推算過。
    pub fn probability(&self, req: &ProbabilityReq) -> Result<ProbabilityResp, String> {
        let cache = self
            .last_guess
            .as_ref()
            .ok_or("還沒有推算結果可以查，先按「計算」")?;
        Ok(req.eval(&cache.candidates, &cache.dist))
    }

    /// 寵物搜尋 —— 原程式的「搜索不低於輸入能力的寵物」。
    pub fn search_by_stats(&self, req: &SearchReq) -> Result<SearchResp, String> {
        check_level(req.lvl)?;
        Ok(req.run(&self.lib.merged))
    }
}

fn check_level(lvl: i32) -> Result<(), String> {
    if (1..=MAX_LEVEL).contains(&lvl) {
        Ok(())
    } else {
        Err(format!("等級 {lvl} 超出 1..={MAX_LEVEL}"))
    }
}

// ── 指令派送 ────────────────────────────────────────────────────────────────

/// 指令參數。欄位名用 camelCase，跟前端送過來的一致。
///
/// 全部塞在同一個結構裡而不是一個指令一個 —— 參數總共就這幾個，
/// 分開寫只會多出十個只用一次的型別。少的欄位是 `None`，用不到就不看。
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Args {
    name: Option<String>,
    csv: Option<String>,
    keyword: Option<String>,
    pet: Option<PetDto>,
    rename_from: Option<String>,
    req: Option<Value>,
}

impl Engine {
    /// 指令派送 —— 跟 Tauri 的 `invoke(cmd, args)` 同一個形狀。
    ///
    /// 參數與回傳都走 JSON，所以整條線可以在沒有瀏覽器的情況下測完。
    pub fn dispatch(&mut self, cmd: &str, args: Value) -> Result<Value, String> {
        let a: Args =
            serde_json::from_value(args).map_err(|e| format!("「{cmd}」的參數看不懂：{e}"))?;

        // 帶 `req` 的指令：把 JSON 解成該指令的型別。
        // 錯誤訊息帶上指令名，不然前端只會看到一句「missing field」。
        macro_rules! req {
            () => {{
                let v = a.req.ok_or_else(|| format!("「{cmd}」少了 req 參數"))?;
                serde_json::from_value(v)
                    .map_err(|e| format!("「{cmd}」的 req 看不懂：{e}"))?
            }};
        }
        macro_rules! need {
            ($f:expr, $n:literal) => {
                $f.ok_or_else(|| format!("「{cmd}」少了 {} 參數", $n))?
            };
        }

        let out = match cmd {
            "bootstrap" => json(&self.bootstrap())?,
            "catalog" => json(&self.catalog())?,
            "load_catalog" => json(&self.load_catalog(&need!(a.csv, "csv"))?)?,
            "reset_catalog" => json(&self.reset_catalog())?,
            "search_pets" => json(&self.search_pets(&need!(a.keyword, "keyword")))?,
            "save_pet" => {
                let pet = need!(a.pet, "pet");
                json(&self.save_pet(pet, a.rename_from.as_deref())?)?
            }
            "delete_pet" => json(&self.delete_pet(&need!(a.name, "name"))?)?,
            "forward" => json(&self.forward(&req!())?)?,
            "series" => json(&self.series(&req!())?)?,
            "guess" => json(&self.guess(&req!())?)?,
            "probability" => json(&self.probability(&req!())?)?,
            "refine" => json(&self.refine(&req!())?)?,
            "search_by_stats" => json(&self.search_by_stats(&req!())?)?,
            _ => return Err(format!("沒有「{cmd}」這個指令")),
        };
        Ok(out)
    }
}

fn json<T: Serialize>(v: &T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| format!("回傳值轉不成 JSON：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json as j;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// 記憶體版的 [`Store`]，行為跟 localStorage 一樣。共用一份 `Rc` 是為了
    /// 讓測試在引擎外面也能看到／竄改存下來的東西。
    #[derive(Default, Clone)]
    struct MemStore(Rc<RefCell<Option<String>>>);

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

    fn engine() -> Engine {
        Engine::new(Box::new(MemStore::default()))
    }

    fn new_pet(name: &str, grow: [i32; 5]) -> Pet {
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
    fn level_bounds_are_enforced() {
        assert!(check_level(1).is_ok());
        assert!(check_level(255).is_ok());
        assert!(check_level(0).is_err());
        assert!(check_level(256).is_err());
    }

    #[test]
    fn builtin_catalog_survives_the_dto_conversion() {
        let dto = engine().catalog();
        assert!(
            dto.count > 700,
            "內建圖鑑只有 {} 隻，TSV 可能沒編進去",
            dto.count
        );
        // 每隻寵物都要落在某個種族分頁裡，不然寵物檔案視窗的分頁會漏東西
        let tabbed: usize = dto.races.iter().map(|r| r.count).sum();
        assert_eq!(tabbed, dto.count);
    }

    /// 兩個 crate 各寫了一份檔次上限（`petdata` 為了維持零相依）。對不上就抓出來。
    #[test]
    fn tier_bounds_agree_with_the_engine() {
        assert_eq!(petdata::MAX_TIER as usize, petcalc::MAX_TIER);
    }

    // ── 自製寵物 ────────────────────────────────────────────────────────────

    #[test]
    fn adding_a_pet_shows_up_in_the_merged_catalog_and_in_the_store() {
        let store = MemStore::default();
        let mut e = Engine::new(Box::new(store.clone()));
        e.lib
            .put(new_pet("我抓的怪", [30, 30, 30, 30, 30]), None, &store)
            .unwrap();

        assert!(e.lib.merged.find_by_name("我抓的怪").unwrap().custom);
        assert_eq!(custom::load(&store).unwrap().len(), 1, "沒存起來");
    }

    /// 改上游圖鑑裡的寵物 ＝ 蓋一層自製的，上游那份原封不動。
    #[test]
    fn editing_a_base_pet_creates_an_override() {
        let mut e = engine();
        let before = e.lib.merged.pets.len();
        e.save_pet(pet_dto("虎人", [99, 1, 1, 1, 1]), None).unwrap();

        assert_eq!(e.lib.merged.pets.len(), before, "覆寫不該多一筆");
        assert_eq!(
            e.lib.merged.find_by_name("虎人").unwrap().grow,
            [99, 1, 1, 1, 1]
        );
        assert_eq!(
            e.lib.builtin.find_by_name("虎人").unwrap().grow,
            [22, 26, 17, 19, 16],
            "上游那份被改到了"
        );
    }

    fn pet_dto(name: &str, grow: [i32; 5]) -> PetDto {
        serde_json::from_value(j!({
            "name": name, "race": 0, "grow": grow, "bprate": 0.2
        }))
        .unwrap()
    }

    /// 刪掉覆寫層，上游那隻要露回來 —— 而不是整隻不見。
    #[test]
    fn deleting_an_override_brings_the_base_pet_back() {
        let mut e = engine();
        e.save_pet(pet_dto("虎人", [99, 1, 1, 1, 1]), None).unwrap();
        e.delete_pet("虎人").unwrap();

        let tiger = e.lib.merged.find_by_name("虎人").expect("上游那隻沒回來");
        assert_eq!(tiger.grow, [22, 26, 17, 19, 16]);
        assert!(!tiger.custom);
    }

    #[test]
    fn deleting_a_base_pet_is_refused() {
        let mut e = engine();
        let err = e.delete_pet("虎人").unwrap_err();
        assert!(err.contains("虎人"), "{err}");
        assert!(e.lib.merged.find_by_name("虎人").is_some(), "被刪掉了");
    }

    /// 名字就是鍵，所以改名要把舊的收掉，不能新舊各留一隻。
    #[test]
    fn renaming_replaces_rather_than_duplicates() {
        let mut e = engine();
        e.save_pet(pet_dto("舊名字", [10, 10, 10, 10, 10]), None)
            .unwrap();
        e.save_pet(pet_dto("新名字", [10, 10, 10, 10, 10]), Some("舊名字"))
            .unwrap();

        assert_eq!(e.lib.custom.len(), 1);
        assert!(e.lib.merged.find_by_name("舊名字").is_none(), "舊名字還在");
        assert!(e.lib.merged.find_by_name("新名字").is_some());
    }

    /// 改名時名字沒真的變（使用者只改了檔次）＝ 單純覆寫，不該把自己刪掉。
    #[test]
    fn saving_without_renaming_keeps_the_pet() {
        let mut e = engine();
        e.save_pet(pet_dto("甲", [10, 10, 10, 10, 10]), None)
            .unwrap();
        e.save_pet(pet_dto("甲", [20, 20, 20, 20, 20]), Some("甲"))
            .unwrap();

        assert_eq!(e.lib.custom.len(), 1);
        assert_eq!(
            e.lib.merged.find_by_name("甲").unwrap().grow,
            [20, 20, 20, 20, 20]
        );
    }

    /// 線上圖鑑的樣本：一隻內建表有的（虎人）＋ 一隻內建表沒有的。
    const ONLINE_CSV: &str = "名稱,種族,體力,力量,防禦,速度,魔法,技格,總檔,屬性\n\
        虎人,龍系,1,2,3,4,5,8,15,地10\n\
        只有線上有,人形系,9,9,9,9,9,8,45,水10\n";

    /// 載入線上圖鑑是**聯集**不是取代：同名的以線上為準，內建獨有的留著。
    ///
    /// 兩邊不是包含關係 —— 線上那份有幾百隻內建表沒有的新寵，內建表也有
    /// 兩百多隻線上那份沒有的舊寵。取代掉任何一邊都會讓一批寵物查不到。
    #[test]
    fn loading_the_online_catalog_is_a_union() {
        let mut e = engine();
        let builtin_only = "貓妖";
        assert!(e.lib.merged.find_by_name(builtin_only).is_some(), "前提壞了");

        e.load_catalog(ONLINE_CSV).unwrap();

        assert_eq!(
            e.lib.merged.find_by_name("虎人").unwrap().grow,
            [1, 2, 3, 4, 5],
            "同名的沒有以線上為準"
        );
        assert!(e.lib.merged.find_by_name("只有線上有").is_some());
        assert!(
            e.lib.merged.find_by_name(builtin_only).is_some(),
            "內建獨有的被線上那份洗掉了"
        );
    }

    /// 載入線上圖鑑不該把使用者自己的東西弄丟。
    #[test]
    fn loading_the_online_catalog_keeps_custom_pets() {
        let mut e = engine();
        e.save_pet(pet_dto("我抓的怪", [30, 30, 30, 30, 30]), None)
            .unwrap();
        e.load_catalog(ONLINE_CSV).unwrap();
        assert!(
            e.lib.merged.find_by_name("我抓的怪").is_some(),
            "自製的不見了"
        );
    }

    /// 線上圖鑑不是自製的 —— 不然整份都會變成可刪除。
    #[test]
    fn the_online_catalog_is_not_custom() {
        let mut e = engine();
        e.load_catalog(ONLINE_CSV).unwrap();
        assert!(!e.lib.merged.find_by_name("只有線上有").unwrap().custom);
        assert!(!e.lib.merged.find_by_name("虎人").unwrap().custom);
    }

    /// 抓不到／解不開的時候只用內建表，不是整個掛掉。
    #[test]
    fn a_broken_catalog_leaves_the_builtin_table_alone() {
        let mut e = engine();
        let before = e.lib.merged.pets.len();
        assert!(e.load_catalog("這不是 CSV").is_err());
        assert_eq!(e.lib.merged.pets.len(), before, "解析失敗不該動到圖鑑");
        assert!(e.lib.merged.find_by_name("虎人").is_some());
    }

    /// `reset_catalog` 把線上那層拿掉，回到只有內建表。
    #[test]
    fn resetting_drops_the_online_layer() {
        let mut e = engine();
        let before = e.lib.merged.pets.len();
        e.load_catalog(ONLINE_CSV).unwrap();
        assert!(e.lib.merged.find_by_name("只有線上有").is_some());

        e.reset_catalog();
        assert_eq!(e.lib.merged.pets.len(), before);
        assert!(e.lib.merged.find_by_name("只有線上有").is_none());
        assert_eq!(
            e.lib.merged.find_by_name("虎人").unwrap().grow,
            [22, 26, 17, 19, 16],
            "內建那筆該回來了"
        );
    }

    /// 驗證不過的東西不該進記憶體，更不該被存起來。
    #[test]
    fn invalid_pets_never_reach_the_store() {
        let store = MemStore::default();
        let mut e = Engine::new(Box::new(store.clone()));
        let err = e
            .save_pet(pet_dto("檔次爆表", [999, 0, 0, 0, 0]), None)
            .unwrap_err();
        assert!(err.contains("999"), "{err}");

        assert!(e.lib.custom.is_empty());
        assert!(e.lib.merged.find_by_name("檔次爆表").is_none());
        assert_eq!(*store.0.borrow(), None, "沒存成功卻寫進去了");
    }

    /// 存失敗要回捲 —— 畫面上顯示存好了、重開卻不見，是最糟的結果。
    #[test]
    fn a_failed_write_rolls_the_memory_state_back() {
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

        let mut e = Engine::new(Box::new(Broken));
        assert!(e
            .save_pet(pet_dto("存不進去", [20, 20, 20, 20, 20]), None)
            .is_err());
        assert!(
            e.lib.merged.find_by_name("存不進去").is_none(),
            "存檔失敗了，圖鑑卻收下了這一筆"
        );
    }

    /// 存檔壞掉不該讓引擎起不來，但錯誤要傳到 `bootstrap`。
    #[test]
    fn a_corrupt_store_still_boots_but_reports() {
        let store = MemStore::default();
        *store.0.borrow_mut() = Some("{ 這不是 JSON".into());
        let e = Engine::new(Box::new(store));

        let boot = e.bootstrap();
        assert!(boot.custom_error.is_some(), "壞掉的存檔被吞了");
        assert!(boot.catalog.count > 700, "圖鑑該照常起來");
    }

    // ── 指令派送 ────────────────────────────────────────────────────────────

    /// 前端會呼叫的每一個指令都要接得起來。
    ///
    /// 這條擋的是「改了 Rust 這邊的名字，前端還在叫舊的」——
    /// 那種錯只會在執行期才炸，而且訊息很難看懂。
    #[test]
    fn every_command_the_front_end_calls_is_wired_up() {
        let mut e = engine();
        for cmd in ["bootstrap", "catalog", "reset_catalog"] {
            e.dispatch(cmd, j!({}))
                .unwrap_or_else(|err| panic!("{cmd}: {err}"));
        }
        e.dispatch("search_pets", j!({ "keyword": "虎" })).unwrap();
        e.dispatch(
            "save_pet",
            j!({
                "pet": { "name": "甲", "race": 0, "grow": [10,10,10,10,10], "bprate": 0.2 },
                "renameFrom": null
            }),
        )
        .unwrap();
        e.dispatch("delete_pet", j!({ "name": "甲" })).unwrap();
        e.dispatch(
            "forward",
            j!({ "req": {
                "grow": {"hp":27,"atk":16,"def":25,"agi":15,"mp":37,"bprate":0.2},
                "lvl": 30, "random": [2,2,2,2,2], "mode": "none",
                "manual": null, "not_order_point": 0, "burst": null
            }}),
        )
        .unwrap();
        e.dispatch(
            "series",
            j!({ "req": {
                "grow": {"hp":27,"atk":16,"def":25,"agi":15,"mp":37,"bprate":0.2},
                "lvl": 1, "random": [2,2,2,2,2], "mode": "none",
                "manual": null, "not_order_point": 0, "burst": null,
                "from": 1, "to": 5
            }}),
        )
        .unwrap();
        e.dispatch(
            "search_by_stats",
            j!({ "req": {
                "lvl": 50, "floor": [400, 300, 0, 0, 0, 0, 0]
            }}),
        )
        .unwrap();
    }

    #[test]
    fn an_unknown_command_says_so() {
        let err = engine().dispatch("沒這個", j!({})).unwrap_err();
        assert!(err.contains("沒這個"), "{err}");
    }

    /// 少參數要講清楚是哪個指令的哪個參數。
    #[test]
    fn a_missing_argument_names_the_command_and_the_field() {
        let err = engine().dispatch("delete_pet", j!({})).unwrap_err();
        assert!(err.contains("delete_pet") && err.contains("name"), "{err}");

        let err = engine().dispatch("forward", j!({})).unwrap_err();
        assert!(err.contains("forward") && err.contains("req"), "{err}");
    }

    /// `renameFrom` 是 camelCase 送過來的 —— Tauri 以前幫忙轉，現在得自己認。
    #[test]
    fn camel_case_argument_names_are_understood() {
        let mut e = engine();
        e.dispatch(
            "save_pet",
            j!({
                "pet": { "name": "舊", "race": 0, "grow": [10,10,10,10,10], "bprate": 0.2 }
            }),
        )
        .unwrap();
        let out = e
            .dispatch(
                "save_pet",
                j!({
                    "pet": { "name": "新", "race": 0, "grow": [10,10,10,10,10], "bprate": 0.2 },
                    "renameFrom": "舊"
                }),
            )
            .unwrap();

        let names: Vec<&str> = out["pets"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|p| p["name"].as_str())
            .collect();
        assert!(names.contains(&"新"));
        assert!(!names.contains(&"舊"), "renameFrom 沒被認出來，舊的還在");
    }

    /// 錯誤走 `Err` 而不是 panic —— wasm 裡 panic 會把整個模組打壞。
    #[test]
    fn errors_come_back_as_errors_not_panics() {
        let mut e = engine();
        assert!(e.dispatch("forward", j!({ "req": { "lvl": 0 } })).is_err());
        assert!(
            e.dispatch(
                "probability",
                j!({ "req": {
                    "lo": [0,0,0,0,0], "hi": [9,9,9,9,9]
                }})
            )
            .is_err(),
            "還沒推算過就查機率該回錯"
        );
    }

    // ── 推算與機率 ──────────────────────────────────────────────────────────

    /// 走完整條 `forward` → `guess` 的線路，回傳引擎與 `guess` 的原始 JSON。
    ///
    /// 刻意用 `Value` 而不是 [`GuessResp`]：那個型別的 `mode` 是 `&'static str`，
    /// 只出不進。用 JSON 反而更貼近前端實際看到的東西。
    fn sample_guess(mode: &str) -> (Engine, Value) {
        let mut e = engine();
        // 先正算 20 級的能力，再拿去反推
        let row = e
            .dispatch(
                "forward",
                j!({ "req": {
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
                j!({ "req": {
                    "grow": {"hp":27,"atk":16,"def":25,"agi":15,"mp":37,"bprate":0.2},
                    "target": {"lvl":20,"hp":s["hp"],"mp":s["mp"],"atk":s["atk"],
                               "def":s["def"],"agi":s["agi"]},
                    "not_order_point": 19, "calc_mode": "smart", "manual": null,
                    "catch_stat": null, "mode": mode, "exhaustive": true,
                    "tolerance": null, "target_grow": null, "limit": 0
                }}),
            )
            .unwrap();
        (e, resp)
    }

    /// 推算的端到端：正算出一組能力，再反推回去要找得到原本那組檔次。
    #[test]
    fn guess_round_trips_a_known_pet() {
        let (_, resp) = sample_guess("exact");
        assert!(resp["total"].as_u64().unwrap() > 0, "推不出任何解");

        let rows = resp["candidates"].as_array().unwrap();
        assert!(
            rows.iter().any(|c| c["grow"] == j!([27, 16, 25, 15, 37])),
            "回傳的 {} 筆解裡沒有原本那組檔次",
            rows.len()
        );
        // 機率是排序鍵，必須單調遞減
        for w in rows.windows(2) {
            let (a, b) = (
                w[0]["percent"].as_f64().unwrap(),
                w[1]["percent"].as_f64().unwrap(),
            );
            assert!(a >= b, "機率沒有由高到低排序：{a} 之後是 {b}");
        }
    }

    #[test]
    fn probability_spans_from_nothing_to_everything() {
        let (e, _) = sample_guess("exact");
        let all = e
            .dispatch_ref(
                "probability",
                j!({ "req": {
                    "lo": [0,0,0,0,0], "hi": [999,999,999,999,999]
                }}),
            )
            .unwrap();
        assert!(
            (all["in_range"].as_f64().unwrap() - 100.0).abs() < 0.01,
            "全範圍應該是 100%：{all}"
        );

        let none = e
            .dispatch_ref(
                "probability",
                j!({ "req": {
                    "lo": [999,999,999,999,999], "hi": [999,999,999,999,999]
                }}),
            )
            .unwrap();
        assert_eq!(none["in_range"].as_f64().unwrap(), 0.0);
        assert!(none["axis_sum"].is_null(), "沒選軸就不該有第二個輸出");
    }

    // ── 再篩選（原程式的「輸入更多資訊」）──────────────────────────────────

    /// 12 格全留空 ＝ 什麼都沒篩，結果必須跟原本那次推算**逐位元相同**。
    ///
    /// 這條是整個功能的地基：不填東西就不該有任何行為改變。
    #[test]
    fn an_empty_filter_reproduces_the_guess_verbatim() {
        let (e, resp) = sample_guess("observer");
        let out = e.dispatch_ref("refine", j!({ "req": {} })).unwrap();

        assert_eq!(out["result"], resp, "留空的篩選改動了結果");
        assert_eq!(out["before"], resp["total"]);
        assert!(
            (out["kept_percent"].as_f64().unwrap() - 100.0).abs() < 0.01,
            "沒篩掉東西，保留比例該是 100%：{}",
            out["kept_percent"]
        );
    }

    /// 一隻掉了檔的野寵：圖鑑 27/16/25/15/37，實際 25/14/23/13/35。
    ///
    /// `sample_guess` 那組沒掉檔、候選只差在隨機檔，12 欄裡幾乎每欄都已經確定
    /// —— 那是測不出「再篩選」的。這一組才有東西可以問。
    fn wild_guess(lvl: i32) -> (Engine, Value) {
        let mut e = engine();
        let row = e
            .dispatch(
                "forward",
                j!({ "req": {
                    "grow": {"hp":25,"atk":14,"def":23,"agi":13,"mp":35,"bprate":0.2},
                    "lvl": lvl, "random": [1,3,2,2,2], "mode": "none",
                    "manual": [0,0,0,0,0], "not_order_point": 0, "burst": null
                }}),
            )
            .unwrap();
        let s = &row["stat"];
        let resp = e
            .dispatch(
                "guess",
                j!({ "req": {
                    "grow": {"hp":27,"atk":16,"def":25,"agi":15,"mp":37,"bprate":0.2},
                    "target": {"lvl":lvl,"hp":s["hp"],"mp":s["mp"],"atk":s["atk"],
                               "def":s["def"],"agi":s["agi"]},
                    "not_order_point": lvl - 1, "calc_mode": "wild", "manual": null,
                    "catch_stat": null, "mode": "observer", "exhaustive": true,
                    "tolerance": null, "target_grow": null, "limit": 0
                }}),
            )
            .unwrap();
        (e, resp)
    }

    /// ⭐ 精神是這 12 格**真的問得出新東西**的證據。
    ///
    /// 推算的輸入只有五項能力（[`TargetDto`] 沒有精神／回復），所以候選之間
    /// 精神是會變的 —— 使用者補上這一格就能砍掉解。這正是原程式那一步的用途，
    /// 也是「為什麼不是把約束全塞在算之前就好」的答案。
    #[test]
    fn the_wisdom_column_is_information_the_query_never_had() {
        let (e, resp) = wild_guess(60);
        assert!(resp["total"].as_u64().unwrap() > 1, "只有一組解，測不出篩選");

        let before = e.dispatch_ref("refine", j!({ "req": {} })).unwrap();
        assert!(
            before["settled"]["stat"][5].is_null(),
            "這個案例的精神已經確定了，換一個案例來測：{}",
            before["settled"]
        );

        let wis = resp["candidates"][0]["stat"]["wis"].as_i64().unwrap();
        let out = e
            .dispatch_ref(
                "refine",
                j!({ "req": { "stat": [null,null,null,null,null,wis,null] } }),
            )
            .unwrap();
        let kept = out["result"]["total"].as_u64().unwrap();
        assert!(kept > 0, "拿候選自己的精神去篩，不該篩到空的");
        assert!(kept < resp["total"].as_u64().unwrap(), "精神沒篩掉任何東西");

        // 篩完之後精神必然「已確定」—— 就是剛剛填進去的那個值。
        assert_eq!(out["settled"]["stat"][5].as_i64(), Some(wis));
        // 保留比例是倖存候選原本權重的總和，不是筆數比例。
        let want: f64 = resp["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["stat"]["wis"].as_i64() == Some(wis))
            .map(|c| c["percent"].as_f64().unwrap())
            .sum();
        assert!(
            (out["kept_percent"].as_f64().unwrap() - want).abs() < 1e-9,
            "保留比例對不上：{} vs {want}",
            out["kept_percent"]
        );
    }

    /// 12 欄全確定 ＝ 原程式收工、不再追問的那一刻。
    ///
    /// ⭐ **那 12 欄是精神／回復 ＋ BP 五軸 ＋ 檔次五軸**，不含生命 魔力 攻擊
    /// 防禦 敏捷 —— 那五格是推算的輸入，原程式從來沒問過（見 [`RefineReq`]）。
    /// 這條測試就填那 12 格，不碰前五格；`all` 若還把它們算進去就會紅。
    ///
    /// 用「照某一組候選填滿」來構造收工狀態，而不是去找一個剛好一步到位的案例
    /// —— 後者換個等級就不成立，前者對任何案例都成立。
    #[test]
    fn every_column_settling_is_how_the_questions_end() {
        let (e, resp) = wild_guess(60);
        let c = &resp["candidates"][0];
        let s = &c["stat"];
        let out = e
            .dispatch_ref(
                "refine",
                j!({ "req": {
                    "stat": [null, null, null, null, null, s["wis"], s["res"]],
                    "bp": c["bp"],
                    "grow": c["grow"]
                }}),
            )
            .unwrap();
        assert_eq!(
            out["settled"]["all"].as_bool(),
            Some(true),
            "12 格都填滿了還說沒定下來：{}",
            out["settled"]
        );
        // `all` 為真時，那 12 欄一格都不能是 null —— 不然畫面會說「問完了」卻還留著空格。
        for col in [5, 6] {
            assert!(!out["settled"]["stat"][col].is_null(), "能力第 {col} 欄");
        }
        for col in out["settled"]["bp"].as_array().unwrap() {
            assert!(!col.is_null());
        }
        for col in out["settled"]["grow"].as_array().unwrap() {
            assert!(!col.is_null());
        }
    }

    /// BP 是這一排最有力的篩選條件 —— 它把加點與隨機檔分得很開，
    /// 而且遊戲的「寵物狀態」視窗直接印這五個數，使用者抄得到。
    ///
    /// ⚠️ 比對走的是 ±0.05（＝ 遊戲那一位小數的半格），不是逐位相同：
    /// 拿候選自己的 BP **四捨五入到一位小數**再填回去，仍然要篩得到它。
    /// 這正是使用者實際會做的事。
    #[test]
    fn the_bp_column_survives_being_read_off_the_game_at_one_decimal() {
        let (e, resp) = wild_guess(60);
        let c = &resp["candidates"][0];
        let rounded: Vec<f64> = c["bp"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| (v.as_f64().unwrap() * 10.0).round() / 10.0)
            .collect();
        let out = e
            .dispatch_ref("refine", j!({ "req": { "bp": rounded } }))
            .unwrap();
        let kept = out["result"]["total"].as_u64().unwrap();
        assert!(kept > 0, "把候選自己的 BP 抄成一位小數填回去，卻篩到空的");
        assert!(kept < resp["total"].as_u64().unwrap(), "BP 沒篩掉任何東西，測不出效果");
    }

    /// `settled` 說某欄確定，就必須真的是所有倖存候選都同一個值 ——
    /// 期望值由掃描算出來，不是寫死的。
    #[test]
    fn a_column_is_settled_exactly_when_every_survivor_agrees() {
        let (e, _) = sample_guess("observer");
        let out = e.dispatch_ref("refine", j!({ "req": {} })).unwrap();
        let cands = out["result"]["candidates"].as_array().unwrap();
        assert!(!out["result"]["truncated"].as_bool().unwrap(), "被截斷就不能拿列表當全集");

        for (col, key) in ["hp", "mp", "atk", "def", "agi", "wis", "res"].iter().enumerate() {
            let vals: Vec<i64> = cands.iter().map(|c| c["stat"][key].as_i64().unwrap()).collect();
            let uniform = vals.iter().all(|v| *v == vals[0]).then_some(vals[0]);
            assert_eq!(out["settled"]["stat"][col].as_i64(), uniform, "能力第 {col} 欄");
        }
        for axis in 0..petcalc::AXES {
            let vals: Vec<i64> = cands
                .iter()
                .map(|c| c["grow"][axis].as_i64().unwrap())
                .collect();
            let uniform = vals.iter().all(|v| *v == vals[0]).then_some(vals[0]);
            assert_eq!(out["settled"]["grow"][axis].as_i64(), uniform, "檔次第 {axis} 軸");

            let bps: Vec<f64> = cands.iter().map(|c| c["bp"][axis].as_f64().unwrap()).collect();
            let uniform = bps.iter().all(|v| (v - bps[0]).abs() < 0.05).then_some(bps[0]);
            assert_eq!(out["settled"]["bp"][axis].as_f64(), uniform, "BP 第 {axis} 軸");
        }
    }

    /// ⭐ 篩完要回頭把掉檔範圍收窄 —— 原程式就是這樣改「搜尋範圍」那一欄的。
    ///
    /// 期望值由**掃全部倖存候選**算出來，不是寫死的。
    /// 另外釘住「不能拿 `lost_marginal` 代替」：那張表只有 0..=4 五格。
    #[test]
    fn the_narrowed_loss_range_scans_every_survivor() {
        let (e, _) = wild_guess(60);
        let out = e.dispatch_ref("refine", j!({ "req": {} })).unwrap();
        let cands = out["result"]["candidates"].as_array().unwrap();
        assert!(!out["result"]["truncated"].as_bool().unwrap(), "被截斷就不能拿列表當全集");

        for axis in 0..petcalc::AXES {
            let lost: Vec<i64> = cands
                .iter()
                .map(|c| c["lost"][axis].as_i64().unwrap())
                .collect();
            let want = [*lost.iter().min().unwrap(), *lost.iter().max().unwrap()];
            let got = &out["narrowed_loss"][axis];
            assert_eq!([got[0].as_i64().unwrap(), got[1].as_i64().unwrap()], want, "第 {axis} 軸");
        }
    }

    /// 篩到空的要老實說空，**不可以**順勢報告「12 欄全確定」——
    /// 那會讓畫面把「條件互斥」演成「問完了」。
    #[test]
    fn filtering_everything_away_is_not_the_same_as_being_done() {
        let (e, _) = sample_guess("observer");
        let out = e
            .dispatch_ref("refine", j!({ "req": { "stat": [999999,null,null,null,null,null,null] } }))
            .unwrap();
        assert_eq!(out["result"]["total"].as_u64(), Some(0));
        assert_eq!(out["settled"]["all"].as_bool(), Some(false));
        assert_eq!(out["kept_percent"].as_f64(), Some(0.0));
        assert!(out["result"]["distribution"].is_null(), "沒有候選就不該有分布");
    }

    /// 沒推算過就篩 —— 回錯，不是回空的。
    #[test]
    fn refining_before_guessing_is_an_error() {
        let e = engine();
        assert!(e.dispatch_ref("refine", j!({ "req": {} })).is_err());
    }

    /// 機率必須用**全部**候選算，不是回傳給前端的那一頁。
    ///
    /// 這條就是 `probability` 得留在後端、不能在前端拿 `resp.candidates`
    /// 自己加總的原因。
    #[test]
    fn probability_counts_every_candidate_not_just_the_returned_page() {
        let (e, resp) = sample_guess("tolerant");
        let total = resp["total"].as_u64().unwrap() as usize;
        assert!(total > 1, "這個案例只有 {total} 組解，測不出差別");

        let full = e
            .dispatch_ref(
                "probability",
                j!({ "req": {
                    "lo": [0,0,0,0,0], "hi": [999,999,999,999,999]
                }}),
            )
            .unwrap();
        assert_eq!(full["candidates"].as_u64().unwrap() as usize, total);
        assert!((full["in_range"].as_f64().unwrap() - 100.0).abs() < 0.01);
    }

    #[test]
    fn axis_sum_probability_is_monotone_in_the_threshold() {
        let (e, _) = sample_guess("tolerant");
        let at = |threshold: i32| {
            e.dispatch_ref(
                "probability",
                j!({ "req": {
                    "lo": [0,0,0,0,0], "hi": [999,999,999,999,999],
                    "axes": [0, 1], "threshold": threshold
                }}),
            )
            .unwrap()["axis_sum"]
                .as_f64()
                .expect("有選軸就該有值")
        };
        assert!((at(0) - 100.0).abs() < 0.01, "門檻 0 應該全中");
        assert!(at(0) >= at(40) && at(40) >= at(999));
        assert_eq!(at(999), 0.0);
    }

    impl Engine {
        /// 只讀的派送，給測試用 —— `probability` 不改狀態，但 `dispatch` 要 `&mut`。
        fn dispatch_ref(&self, cmd: &str, args: Value) -> Result<Value, String> {
            match cmd {
                "probability" => {
                    let req = serde_json::from_value(args["req"].clone()).unwrap();
                    json(&self.probability(&req)?)
                }
                "refine" => {
                    let req = serde_json::from_value(args["req"].clone()).unwrap();
                    json(&self.refine(&req)?)
                }
                _ => unreachable!("dispatch_ref 只給唯讀指令用"),
            }
        }
    }
}
