// 「當前能力」欄位的解析。
//
// 原程式這格的提示字是「像論壇發貼一樣的輸入格式」—— 也就是說使用者會直接
// 從論壇／遊戲裡複製一串亂七八糟的東西貼進來。所以這裡刻意寬鬆：
//
//   `1234 567 89 90 100`            純數字，順序＝血魔攻防敏
//   `1234/567/89/90/100 (110 120)`  任何非數字都當分隔符，括號裡是精神／回復
//   `生命1234 魔力567 攻擊89 ...`    有標籤就照標籤認，順序隨便
//   `LV50 生命1234 ...`              標籤模式下順便把等級也撿回來
//
// 有標籤就用標籤（比較可靠），完全沒標籤才退回位置對應。

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
 * @returns `{ ok, values, lvl, matched, extra }`
 *   `values` 是 `{hp,mp,atk,def,agi,wis,res}`（沒認出來的是 null），
 *   `matched` 是認出幾個能力，`extra` 是多出來沒用到的數字個數。
 */
export function parseStats(text) {
    const blank = { ok: false, values: emptyValues(), lvl: null, matched: 0, extra: 0 };
    if (!text || !text.trim()) return blank;

    const labelled = parseLabelled(text);
    if (labelled.matched > 0) return labelled;

    const nums = allNumbers(text);
    if (nums.length === 0) return blank;

    const values = emptyValues();
    const take = Math.min(nums.length, 5);
    for (let i = 0; i < take; i++) values[STAT_KEYS[i]] = nums[i];
    // 第 6、7 個數字才是精神／回復，而且要成對才算數 —— 只多一個通常是雜訊
    if (nums.length >= 7) {
        values.wis = nums[5];
        values.res = nums[6];
    }
    const matched = STAT_KEYS.filter((k) => values[k] !== null).length;
    return { ok: matched >= 5, values, lvl: null, matched, extra: nums.length - matched };
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
