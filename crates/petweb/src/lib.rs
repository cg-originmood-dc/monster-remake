//! 「噬生・魔物觀測者」remake 的 wasm 綁定。
//!
//! 這裡**沒有邏輯** —— 全部在 [`petcore`] 裡，那樣才能用普通的 `cargo test` 測。
//! 這層只做三件事：型別在 JS 與 Rust 之間轉換、把錯誤變成 JS 例外、
//! 告訴外面有沒有東西要存起來。
//!
//! # 為什麼持久化不在這裡做
//!
//! 這個模組跑在 **Web Worker** 裡（推算最壞情況要跑好幾秒，擺在主執行緒
//! 會把畫面凍住）。而 Worker **拿不到 `localStorage`** —— 那是規格明訂的，
//! 不是 bug。IndexedDB 拿得到，但它是非同步的，塞不進
//! [`Store::write`](petcore::custom::Store::write) 這種同步介面。
//!
//! 所以分工是：wasm 這邊把自製寵物存在記憶體裡並標記「有變動」，
//! 主執行緒每次呼叫完問一下 [`Observer::take_dirty`]，有東西就寫進
//! `localStorage`。**JSON 的內容仍然完全由 Rust 產生**，主執行緒只是搬運工，
//! 不會有兩份格式各自演化的問題。

use std::cell::RefCell;
use std::rc::Rc;

use serde::Serialize;
use wasm_bindgen::prelude::*;

use petcore::custom::Store;
use petcore::Engine;

/// 記憶體裡的自製寵物，外加一個「還沒被拿走的變動」旗標。
///
/// `Rc` 是因為 [`Engine`] 會把 [`Store`] 收走（`Box<dyn Store>`），
/// 但 [`Observer`] 還得看得到那個旗標。
#[derive(Default, Clone)]
struct MemStore(Rc<RefCell<Slot>>);

#[derive(Default)]
struct Slot {
    text: Option<String>,
    /// 上次 [`Observer::take_dirty`] 之後有沒有被寫過。
    dirty: bool,
}

impl Store for MemStore {
    fn read(&self) -> Result<Option<String>, String> {
        Ok(self.0.borrow().text.clone())
    }

    fn write(&self, text: &str) -> Result<(), String> {
        let mut slot = self.0.borrow_mut();
        slot.text = Some(text.to_string());
        slot.dirty = true;
        Ok(())
    }

    fn location(&self) -> String {
        // 顯示給使用者看的位置。桌面版這裡是檔案路徑。
        "瀏覽器本機儲存（localStorage）".into()
    }
}

/// 引擎的 JS 門面。
#[wasm_bindgen]
pub struct Observer {
    engine: Engine,
    store: MemStore,
}

#[wasm_bindgen]
impl Observer {
    /// 建立引擎。
    ///
    /// * `custom_json` —— 主執行緒從 `localStorage` 讀出來的那串，沒有就傳 `null`
    ///
    /// 圖鑑先用編進 wasm 的內建表；線上那份由前端抓下來之後走
    /// `invoke("load_catalog", { csv })` 疊上去。
    ///
    /// 存檔壞掉**不會**讓這裡失敗：先當成沒有自製寵物，錯誤留到
    /// `bootstrap` 回報給前端。整個程式因為一份壞掉的存檔而起不來是最糟的結果。
    #[wasm_bindgen(constructor)]
    pub fn new(custom_json: Option<String>) -> Result<Observer, JsValue> {
        #[cfg(feature = "console_error_panic_hook")]
        console_error_panic_hook::set_once();

        let store = MemStore::default();
        if let Some(text) = custom_json {
            // 直接塞進去，不走 write() —— 這是「讀進來的」，不是變動，
            // 不能讓它一開機就被當成待存的東西寫回去。
            store.0.borrow_mut().text = Some(text);
        }

        Ok(Observer {
            engine: Engine::new(Box::new(store.clone())),
            store,
        })
    }

    /// 指令派送 —— 跟 Tauri 的 `invoke(cmd, args)` 同一個形狀。
    pub fn invoke(&mut self, cmd: &str, args: JsValue) -> Result<JsValue, JsValue> {
        let args: serde_json::Value = if args.is_undefined() || args.is_null() {
            serde_json::Value::Object(Default::default())
        } else {
            serde_wasm_bindgen::from_value(args)
                .map_err(|e| err(&format!("「{cmd}」的參數轉不過來：{e}")))?
        };

        let out = self.engine.dispatch(cmd, args).map_err(|e| err(&e))?;

        // json_compatible：預設會把 map 轉成 JS 的 Map，前端要的是普通物件。
        out.serialize(&serde_wasm_bindgen::Serializer::json_compatible())
            .map_err(|e| err(&format!("「{cmd}」的結果轉不過來：{e}")))
    }

    /// 有沒有自製寵物要存？有的話回那串 JSON，並清掉旗標。
    ///
    /// 主執行緒每次 [`invoke`](Self::invoke) 之後問一次；回 `null` 就是沒事。
    #[wasm_bindgen(js_name = takeDirty)]
    pub fn take_dirty(&mut self) -> Option<String> {
        let mut slot = self.store.0.borrow_mut();
        if !slot.dirty {
            return None;
        }
        slot.dirty = false;
        slot.text.clone()
    }
}

fn err(msg: &str) -> JsValue {
    js_sys::Error::new(msg).into()
}
