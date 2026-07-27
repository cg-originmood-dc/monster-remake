// 跟計算核心的唯一接觸面。
//
// 每個函式對應 `crates/petcore` 的一個指令，參數形狀對應 `dto.rs`。
// **軸順序一律是 [體力,力量,強度,速度,魔法]**，只有 stat（血魔攻防敏精神回復）
// 是另一個順序 —— 見 dto.rs 的檔頭。
//
// 計算跑在 Web Worker 裡的 wasm。這一層負責三件 Worker 做不到的事：
// 開 worker、配對請求與回應、把自製寵物寫進 localStorage。

/** 自製寵物在 localStorage 的鍵。跟桌面版的檔名一致，內容也是同一份 JSON。 */
const STORAGE_KEY = 'custom_pets.json';

/**
 * 線上圖鑑：originmood 的「專屬寵物」CSV（1069 筆）。
 *
 * 這份**不進版控**：要的是「永遠跟著上游走」，不是某次轉檔的快照。
 * 代價是抓不到就只剩內建表（少了幾百隻新寵，但程式照樣能用）——
 * `bootstrap()` 因此不會讓這一步的失敗擴散出去。
 *
 * ## 為什麼有兩條
 *
 * 依序試，第一條成功就用。**不是**兩份資料 —— 是同一個檔案的兩個出口
 * （站台那條是 build 時從 `content/data/` 讀出來原樣吐的，逐位元相同）。
 *
 * 1. **站台的 Pages**：對方 repo 的 `src/pages/data/[name].csv.ts` 發布的。
 *    本站若也掛在 `cg-originmood-dc.github.io` 底下，這條是**同源**，
 *    完全不吃 CORS；而且是站台自己承諾的公開路徑，比戳進來源樹穩。
 * 2. **`raw.githubusercontent.com`**：來源檔本人。站台還沒部署新版、
 *    或哪天路由被拿掉時的退路。這個 host 回 `Access-Control-Allow-Origin: *`。
 *
 * 兩條都掛才會退回內建表。
 */
const CATALOG_URLS = [
    'https://cg-originmood-dc.github.io/data/%E5%B0%88%E5%B1%AC%E5%AF%B5%E7%89%A9.csv',
    'https://raw.githubusercontent.com/cg-originmood-dc/cg-originmood-dc.github.io' +
        '/main/content/data/%E5%B0%88%E5%B1%AC%E5%AF%B5%E7%89%A9.csv',
];

let worker = null;
let seq = 0;
const pending = new Map();

/**
 * localStorage 在無痕模式、或使用者關掉本機儲存時會直接丟例外。
 * 那種情況不該讓整個程式起不來 —— 退成「這次不會存起來」就好。
 */
function storage() {
    try {
        // 光是取用 window.localStorage 就可能丟，所以連讀都要包起來
        const s = window.localStorage;
        s.getItem(STORAGE_KEY);
        return s;
    } catch {
        return null;
    }
}

function send(cmd, args, boot) {
    return new Promise((resolve, reject) => {
        const id = ++seq;
        pending.set(id, { resolve, reject });
        worker.postMessage({ id, cmd, args, boot });
    });
}

function onMessage({ data: { id, ok, value, error, persist } }) {
    const slot = pending.get(id);
    if (!slot) return;
    pending.delete(id);

    // 自製寵物有變動就寫回去。寫失敗只警告不擋 ——
    // 資料在記憶體裡還是對的，硬把成功的操作報成失敗更難懂。
    if (persist != null) {
        try {
            storage()?.setItem(STORAGE_KEY, persist);
        } catch (e) {
            console.warn('自製寵物存不進 localStorage：', e);
        }
    }

    ok ? slot.resolve(value) : slot.reject(new Error(error));
}

function invoke(cmd, args) {
    if (!worker) return Promise.reject(new Error(`「${cmd}」在引擎啟動前就被呼叫了`));
    return send(cmd, args);
}

/**
 * 啟動：開 worker、載入圖鑑清單、把 localStorage 裡的自製寵物交給引擎，
 * 然後回一份 bootstrap（圖鑑 ＋ 常數）。
 */
export async function bootstrap() {
    if (!worker) {
        worker = new Worker(new URL('./worker.js', import.meta.url), { type: 'module' });
        worker.onmessage = onMessage;
        worker.onerror = (e) => {
            // worker 整個掛掉的話，所有等在那裡的請求都不會有回應了
            const err = new Error(`計算核心異常：${e.message ?? e}`);
            pending.forEach(({ reject }) => reject(err));
            pending.clear();
        };
    }

    await send('__init', null, {
        custom: storage()?.getItem(STORAGE_KEY) ?? null,
    });

    // 線上圖鑑抓不到不是致命傷 —— 內建表是編在 wasm 裡的，照樣能算。
    // 所以這裡吞掉錯誤，讓 bootstrap 一定回得來。
    try {
        await loadCatalog();
    } catch (e) {
        console.warn('線上圖鑑載入失敗，只能用內建圖鑑：', e);
    }
    return invoke('bootstrap');
}

/**
 * 抓線上圖鑑疊到內建表上（同名以線上為準，內建獨有的留著）。
 *
 * **解析在 Rust 那邊做** —— CSV 的形狀由 `petdata::parse_originmood` 說了算，
 * 前端只把位元組搬過去，不該有第二套認知。
 */
export async function loadCatalog() {
    const failures = [];
    for (const url of CATALOG_URLS) {
        try {
            const res = await fetch(url);
            if (!res.ok) throw new Error(`HTTP ${res.status}`);
            return await invoke('load_catalog', { csv: await res.text() });
        } catch (e) {
            // 第一條掛掉不值得吵 —— 還有下一條。全掛了才把整串理由丟出去。
            failures.push(`${url} → ${e?.message ?? e}`);
        }
    }
    throw new Error(`載不到線上圖鑑：\n${failures.join('\n')}`);
}

/** 拿掉線上那層，只用內建表。 */
export const resetCatalog = () => invoke('reset_catalog');

export const searchPets = (keyword) => invoke('search_pets', { keyword });

/**
 * 新增或修改一隻自製寵物，回傳更新後的整份圖鑑。
 *
 * `renameFrom` 是改名前的舊名字 —— 名字就是鍵，不告訴引擎的話舊的那筆會留著。
 * 改的若是上游圖鑑裡的寵物，會變成一筆同名的自製記錄蓋在上面，
 * 上游那份永遠不會被寫到。
 */
export const savePet = (pet, renameFrom = null) => invoke('save_pet', { pet, renameFrom });

/** 刪掉一隻自製寵物。蓋掉上游的那種，刪完上游那隻會露回來。 */
export const deletePet = (name) => invoke('delete_pet', { name });

/** 單一等級的正算。 */
export const forward = (req) => invoke('forward', { req });

/** 一段等級區間的正算（模擬分頁的成長表）。 */
export const series = (req) => invoke('series', { req });

/** 推算：由能力反推檔次／加點／隨機檔。這支可能跑幾秒，所以才要 worker。 */
export const guess = (req) => invoke('guess', { req });

/**
 * 機率查詢（原程式的機率統計）—— 算在**最後一次 [`guess`]** 的候選上，
 * 所以要先推算過。
 *
 * 用引擎而不是拿 `guess` 回來的 `candidates` 自己加總，是因為那份被截斷過；
 * 引擎留著完整的候選與權重。
 */
export const probability = (req) => invoke('probability', { req });

/**
 * 推算後的再篩選（原程式的「輸入更多資訊」）—— 12 個框：7 欄能力 ＋ 5 欄檔次。
 *
 * 跟 [`probability`] 一樣掃**最後一次 [`guess`]** 的完整候選，而且**不重算**：
 * 原程式也是這樣，候選表已經建好了，補資訊只是再掃一遍把不合的砍掉。
 * 所以邊填邊看是即時的，不必等好幾秒重推一次。
 */
export const refine = (req) => invoke('refine', { req });

/** 寵物搜尋（原程式主視窗的寵物搜尋面板）：找出在指定等級補得到這組能力的寵物。 */
export const searchByStats = (req) => invoke('search_by_stats', { req });

/**
 * 把自製寵物整份倒出來，讓使用者自己存檔。
 *
 * 網頁版的資料在 localStorage 裡，清瀏覽器資料就沒了 —— 桌面版至少還有個
 * 檔案躺在硬碟上。倒出來的格式跟桌面版的 `custom_pets.json` 一模一樣。
 */
export const exportCustom = () => storage()?.getItem(STORAGE_KEY) ?? null;

/**
 * 把倒出去的那份倒回來，取代目前的自製寵物。
 *
 * 走 `localStorage` → 重跑 bootstrap，而不是逐隻 `savePet`：那樣才會經過
 * Rust 那邊完整的格式驗證，壞掉的檔案會在載入時就被擋下來。
 */
export async function importCustom(json) {
    const s = storage();
    if (!s) throw new Error('這個瀏覽器不讓存資料（無痕模式？），沒辦法匯入');

    const before = s.getItem(STORAGE_KEY);
    s.setItem(STORAGE_KEY, json);
    try {
        return await bootstrap();
    } catch (e) {
        // 匯入的東西壞掉就還原，不要把使用者本來好好的資料弄丟
        before == null ? s.removeItem(STORAGE_KEY) : s.setItem(STORAGE_KEY, before);
        throw e;
    }
}
