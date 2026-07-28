// 網址帶寵物 —— ⚠️ **原程式沒有這個，是移植版加的**（CLAUDE.md §4.0 硬規則 2）。
//
// 原程式是桌面 exe，根本沒有網址這種東西可以帶參數進來。移植版是網頁，
// 「把一隻寵物連同數字丟給別人看」最自然的形狀就是一條連結：
//
//     .../?q=衝浪小黃鴨 114 77 50 40 31     開起來就是填好的，直接按「計算」
//     .../?q=小白鴨                         只載寵物：檔次、能力倍率填好，等級歸 1
//
// ## 為什麼只有一個參數
//
// `q` 的內容**就是「當前能力」那一格吃的同一串**（格式見 `parse.js` 檔頭），
// 一條解析規則都不必另外定。刻意**不**做 `?pet=&lvl=&stat=` 那種拆開的參數：
// 那會變成第二套規則，跟那一格的行為遲早分家 —— 而且那一串本來就是使用者
// 手上會有的東西（Discord「/算檔」機器人的指令格式），拆開反而要他重打一次。
//
// 於是「網址帶寵物」在應用層是零行程式碼：`statText` 一填，既有的那條
// 「認出寵物名 → `pickPet` → 檔次／倍率／等級一起帶進來」的路（`app.js`）
// 就自己跑完了。
//
// ## 為什麼是 replaceState
//
// 位址列跟著那一格走，使用者才複製得到連結（不必自己拼參數）。用 `replaceState`
// 不是 `pushState`：每敲一個字推一筆歷史的話，按「上一頁」會在同一個畫面裡
// 倒退幾十次，等於離不開這一頁。

/** 查詢參數名。短是刻意的 —— 這條連結是要貼給人的。 */
const PARAM = 'q';

/** 開網頁時網址帶進來的那串；沒有就空字串。 */
export function readShareLink() {
    if (typeof location === 'undefined') return '';
    try {
        return new URLSearchParams(location.search).get(PARAM)?.trim() ?? '';
    } catch {
        // 網址壞掉不該讓整個程式開不起來 —— 當作沒帶參數就好。
        return '';
    }
}

/** 把目前的查詢寫回位址列，讓使用者直接複製網址就是一條可以分享的連結。 */
export function writeShareLink(text) {
    if (typeof location === 'undefined' || typeof history === 'undefined') return;
    try {
        const url = new URL(location.href);
        if (text) url.searchParams.set(PARAM, text);
        else url.searchParams.delete(PARAM);
        // 沒變就不要寫 —— 每次 render 都呼叫一次 replaceState 是白花的。
        if (url.href !== location.href) history.replaceState(null, '', url.href);
    } catch {
        // 分享連結壞掉是小事，不值得把畫面弄掛。
    }
}
