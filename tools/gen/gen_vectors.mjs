// 產生 Rust 計算核心的對照向量。
//
// 以 cg-pet-calc 的 GrowRange/BP 作為正向計算的 baseline。
//
// 歷史：cg-pet-calc 的 fullRates 原本只到 index 52，且 51/52 的值是錯的
// （2.04 / 2.08，正確為 2.14 / 2.18），所以這裡曾經只敢對照到檔次 50。
// 該表已依噬生・魔物觀測者 v3.12 的完整表修正成 111 筆，
// 現在**整個檔次範圍 0..110 都必須一致**。
//
// ⚠️ cg-pet-calc 的 Stat 建構子是 (lvl, hp, mp, attack, defend, agi) 只有 6 個參數，
//    但 calcRealNum() 傳了 7 個值進去 —— **精神(wis) 與 回復(res) 被靜默丟棄**。
//    所以對照只涵蓋 血魔攻防敏 五項；wis/res 由 Rust 端自行以矩陣測試驗證。
//
// 輸出格式（每行一筆，`|` 分隔，避免 Rust 端引入 JSON 相依）：
//   hp,atk,def,agi,mp | bprate | lvl | hp,mp,atk,def,agi | sumBaseBP | sumFullBP
//
// 用法： node tools/gen/gen_vectors.mjs
//
// ⚠️ cg-pet-calc **刻意不列在 package.json 的相依裡**。它是 `file:../cg-pet-calc`
//    的本機路徑，GitHub Actions 的 checkout 裡沒有那個目錄，留著會讓 `npm ci` 直接掛掉。
//    產出的向量已簽入 `crates/petcalc/tests/vectors/`，所以 `cargo test` 與網頁建置都不需要它。
//    要重產向量時自己裝一次：
//        npm i --no-save file:../cg-pet-calc

import { GrowRange } from 'cg-pet-calc/lib/Pets.mjs';
import { fullRates } from 'cg-pet-calc/lib/Utils.mjs';
import { writeFileSync } from 'node:fs';

const MAX_TIER = 110; // 成長係數表上限（噬生・魔物觀測者 v3.12）

// 先確認對照組本身沒退化 —— 如果 fullRates 又被改壞，
// 產生出來的向量就是垃圾，寧可在這裡直接爆掉。
for (let n = 0; n <= MAX_TIER; n++) {
    if (typeof fullRates[n] !== 'number') {
        throw new Error(`cg-pet-calc 的 fullRates[${n}] 不是數字 —— 表被截斷了？`);
    }
}
if (fullRates[51] !== 2.14 || fullRates[52] !== 2.18 || fullRates[110] !== 4.615) {
    throw new Error('cg-pet-calc 的 fullRates 值不對，先修好它再產生向量');
}

function emit(tiers, bprate, lvl) {
    const [hp, atk, def, agi, mp] = tiers;
    const g = new GrowRange(hp, atk, def, agi, mp, bprate);
    const r = g.calcBPAtLevel(lvl, null);
    const s = r.baseBP.calcRealNum();
    const stats = [s.hp, s.mp, s.attack, s.defend, s.agi];
    if (stats.some((n) => typeof n !== 'number' || Number.isNaN(n))) {
        throw new Error(`算出 NaN: tiers=${tiers} bprate=${bprate} lvl=${lvl}`);
    }
    return [tiers.join(','), bprate, lvl, stats.join(','), r.sumBaseBP, r.sumFullBP].join('|');
}

// 決定性的偽亂數，讓向量可重現
let seed = 20230611;
const rnd = (n) => {
    seed = (seed * 1103515245 + 12345) & 0x7fffffff;
    return seed % n;
};

const lines = [];
const push = (l) => {
    if (l) lines.push(l);
};

// 1. 邊界：全 0、全滿檔、單軸突出
for (const lvl of [1, 2, 10, 50, 100, 150, 255]) {
    push(emit([0, 0, 0, 0, 0], 0.2, lvl));
    push(emit([MAX_TIER, MAX_TIER, MAX_TIER, MAX_TIER, MAX_TIER], 0.2, lvl));
    for (let axis = 0; axis < 5; axis++) {
        const t = [0, 0, 0, 0, 0];
        t[axis] = MAX_TIER;
        push(emit(t, 0.2, lvl));
    }
}

// 2. 每個檔次值都至少被覆蓋一次（放在 HP 軸上）
for (let tier = 0; tier <= MAX_TIER; tier++) {
    push(emit([tier, 3, 5, 7, 9], 0.2, 60));
}

// 3. 曾經壞掉的區段：51..110 每一檔都在多個等級下對照
for (let tier = 51; tier <= MAX_TIER; tier++) {
    for (const lvl of [1, 50, 100, 200]) {
        push(emit([tier, tier, tier, tier, tier], 0.2, lvl));
    }
}

// 4. 亂數組合，涵蓋不同 bprate
for (const bprate of [0.2, 0.15, 0.25, 0.3]) {
    for (let i = 0; i < 200; i++) {
        const tiers = [
            rnd(MAX_TIER + 1),
            rnd(MAX_TIER + 1),
            rnd(MAX_TIER + 1),
            rnd(MAX_TIER + 1),
            rnd(MAX_TIER + 1),
        ];
        push(emit(tiers, bprate, 1 + rnd(255)));
    }
}

writeFileSync(new URL('../vectors/forward.txt', import.meta.url), lines.join('\n') + '\n', 'utf8');
console.log(`wrote ${lines.length} agreement vectors (tier 0..${MAX_TIER})`);
