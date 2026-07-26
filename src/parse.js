// 「當前能力」欄位的解析。
//
// 原程式這格的提示字是「像論壇發貼一樣的輸入格式」—— 也就是說使用者會直接
// 從論壇／遊戲裡複製一串亂七八糟的東西貼進來。所以這裡刻意寬鬆：
//
//   `1234 567 89 90 100`            純數字，順序＝血魔攻防敏
//   `1234/567/89/90/100 (110 120)`  任何非數字都當分隔符，括號裡是精神／回復
//   `生命1234 魔力567 攻擊89 ...`    有標籤就照標籤認，順序隨便
//   `LV50 生命1234 ...`              標籤模式下順便把等級也撿回來
//   `幽紫妖靈 99 133 32 38 36`       開頭是圖鑑裡的寵物名 → 連檔次一起帶進來
//
// 有標籤就用標籤（比較可靠），完全沒標籤才退回位置對應。
//
// ## 寵物名開頭：`寵 等級(可省略) 血 魔 攻 防 敏`
//
// 這是 Discord「/算檔」機器人的指令格式。要支援它有兩個坑：
//
// **① 不能「切到第一個數字」當名字。** 圖鑑裡有 `22週年鼠王`、`機甲D4型`、
// `第16期金怪` 這種名字 —— 切下去會得到空字串或 `機甲D`。所以走
// **最長的完全相符前綴**：拿整份圖鑑去比對誰是這串文字的開頭，取最長的那個。
// 這也正好落實「前提是有完全相符的名字」—— 認不出來就當作沒有名字，
// 照舊走純數字那條，不會亂猜。
//
// **② 等級靠數字個數判斷。** 能力本體是 5 個（血魔攻防敏）或 7 個（多精神回復），
// 兩個都是奇數，所以 **6 個或 8 個 → 第一個是等級**，不會有歧義。這個規則只在
// 認出寵物名時才套用，沒有名字的輸入行為完全不變（6 個數字照舊只取前 5 個）。

/** 能力的欄位順序 —— 與 `dto.rs` 的 `StatDto` 一致。 */
export const STAT_KEYS = ['hp', 'mp', 'atk', 'def', 'agi', 'wis', 'res'];
export const STAT_NAMES = ['生命', '魔力', '攻擊', '防禦', '敏捷', '精神', '回復'];

// 一個欄位可能有好幾種寫法；長的別名要排在短的前面，才不會被短的先吃掉
// （「魔力」必須贏過「魔」，否則「魔法」也會被認成魔力）。
const ALIASES = [
    ['lvl', ['等級', '等 級', 'LV', 'Lv', 'lv']],
    ['hp', ['生命力', '生命', '體力', 'HP', 'hp']],
    ['mp', ['魔力', '魔法力', 'MP', 'mp']],
    ['atk', ['攻擊力', '攻擊', '攻击', '力量', 'ATK', 'atk']],
    ['def', ['防禦力', '防禦', '防御', '強度', 'DEF', 'def']],
    ['agi', ['敏捷度', '敏捷', '速度', 'AGI', 'agi']],
    ['wis', ['精神力', '精神', 'WIS', 'wis']],
    ['res', ['回復力', '回復', '恢復', 'RES', 'res']],
];

/**
 * 解析一行能力字串。
 *
 * @param text 使用者貼進來的東西
 * @param pets 圖鑑（`catalog.pets`）。給了才會去認開頭的寵物名；不給就完全照舊。
 * @returns `{ ok, values, lvl, matched, extra, pet }`
 *   `values` 是 `{hp,mp,atk,def,agi,wis,res}`（沒認出來的是 null），
 *   `matched` 是認出幾個能力，`extra` 是多出來沒用到的數字個數，
 *   `pet` 是認出來的圖鑑那一筆（沒有就 null）。
 */
export function parseStats(text, pets) {
    const blank = {
        ok: false,
        values: emptyValues(),
        lvl: null,
        matched: 0,
        extra: 0,
        pet: null,
    };
    if (!text || !text.trim()) return blank;

    // 名字先拆掉再解析：留著的話「魔力鳥」這種名字有機會被標籤模式當成「魔力」。
    const named = matchPetName(text, pets);
    const body = named ? named.rest : text;
    const withPet = (r) => ({ ...r, pet: named?.pet ?? null });

    const labelled = parseLabelled(body);
    if (labelled.matched > 0) return withPet(labelled);

    let nums = allNumbers(body);
    if (nums.length === 0) return withPet(blank);

    let lvl = null;
    // 只有認出寵物名時才吃這條 —— 見檔頭 ②。5/7 是能力本體，6/8 才是前面多一個等級。
    if (named && (nums.length === 6 || nums.length === 8)) {
        lvl = nums[0];
        nums = nums.slice(1);
    }

    const values = emptyValues();
    const take = Math.min(nums.length, 5);
    for (let i = 0; i < take; i++) values[STAT_KEYS[i]] = nums[i];
    // 第 6、7 個數字才是精神／回復，而且要成對才算數 —— 只多一個通常是雜訊
    if (nums.length >= 7) {
        values.wis = nums[5];
        values.res = nums[6];
    }
    const matched = STAT_KEYS.filter((k) => values[k] !== null).length;
    return withPet({ ok: matched >= 5, values, lvl, matched, extra: nums.length - matched });
}

/**
 * 在文字**開頭**找出圖鑑裡的寵物名 —— 取**最長的完全相符前綴**。
 *
 * 不是「切到第一個數字」：`22週年鼠王` 會被切成空字串、`機甲D4型` 會被切成 `機甲D`。
 * 也不做模糊比對 —— 使用者要的是「有完全相符的名字」才算數，認不出來就當沒名字。
 *
 * @returns `{ pet, rest }`，`rest` 是名字後面剩下的文字；認不出來回 `null`。
 */
function matchPetName(text, pets) {
    if (!pets?.length) return null;
    const head = text.trimStart();

    let best = null;
    for (const p of pets) {
        if (!p.name || p.name.length <= (best?.name.length ?? 0)) continue;
        if (head.startsWith(p.name)) best = p;
    }
    if (!best) return null;
    // 純數字的名字要擋掉，否則「99 133 32 38 36」這種正常輸入會被當成寵物名。
    // 只檢查最長的那筆就夠 —— 更短的相符名必然是它的前綴，也必然是純數字。
    if (/^\d+$/.test(best.name)) return null;

    return { pet: best, rest: head.slice(best.name.length) };
}

function parseLabelled(text) {
    const values = emptyValues();
    let lvl = null;
    let matched = 0;

    for (const [key, names] of ALIASES) {
        for (const name of names) {
            // 標籤後面允許 ：: = 或空白，然後才是數字
            const re = new RegExp(`${escapeRe(name)}\\s*[：:=]?\\s*(\\d+)`);
            const m = text.match(re);
            if (!m) continue;
            const n = Number(m[1]);
            if (key === 'lvl') lvl = n;
            else {
                values[key] = n;
                matched++;
            }
            break;
        }
    }
    return { ok: matched >= 5, values, lvl, matched, extra: 0 };
}

/** 把全形數字換成半形之後，抓出所有非負整數。 */
function allNumbers(text) {
    const half = text.replace(/[０-９]/g, (c) => String.fromCharCode(c.charCodeAt(0) - 0xfee0));
    return (half.match(/\d+/g) || []).map(Number);
}

function emptyValues() {
    return Object.fromEntries(STAT_KEYS.map((k) => [k, null]));
}

function escapeRe(s) {
    return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/** 反向：把一組能力印回論壇格式，讓「複製回去」也能用。 */
export function formatStats(stat) {
    if (!stat) return '';
    const main = ['hp', 'mp', 'atk', 'def', 'agi'].map((k) => stat[k]).join(' ');
    if (stat.wis == null && stat.res == null) return main;
    return `${main} (${stat.wis} ${stat.res})`;
}
