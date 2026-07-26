// 產生**推算**（反推）的對照向量。
//
// gen_vectors.mjs 只對照正向計算；這支對照的是 GrowRange.guess() ——
// 也就是「由觀測能力反推 (檔次, 加點, 隨機檔)」那條路徑，
// 是移植中最容易出現細微行為差異的地方。
//
// 對照的是 cg-pet-calc 的**精確整數列舉**路徑，所以 Rust 端要用
// MatchMode::Exact + RandomSearch::Ball{radius:1} 才是同一個演算法。
// （噬生・魔物觀測者 v3.12 的容差模式本來就會找到更多解，那是刻意的差異，不在這裡比。）
//
// 輸出格式：
//   CASE|標籤|檔次 hp,atk,def,agi,mp|bprate|lvl|能力 hp,mp,atk,def,agi|notorderpoint|解數
//   SOL |檔次 hp,atk,def,agi,mp|加點 a,b,c,d,e|隨機檔 r0..r4|isApproximate
//
// 用法： node tools/gen/gen_guess_vectors.mjs

import { GrowRange, Stat, BP } from 'cg-pet-calc/lib/Pets.mjs';
import { PetDefaultData } from 'cg-pet-calc/index.js';
import { writeFileSync } from 'node:fs';

const lines = [];

function emitCase(label, tiers, bprate, lvl, stats, notorderpoint) {
    const rng = new GrowRange(...tiers, bprate);
    const stat = new Stat(lvl, ...stats);
    const results = rng.guess(stat, notorderpoint);

    lines.push(
        [
            'CASE',
            label,
            tiers.join(','),
            bprate,
            lvl,
            stats.join(','),
            notorderpoint,
            results.length,
        ].join('|'),
    );

    // 排序讓輸出穩定，不受列舉順序的實作細節影響
    const rows = results.map((r) =>
        [
            r.GuessRange.toArray().join(','),
            r.ManualPoints.join(','),
            r.RandomRange.join(','),
            r.isApproximate ? 1 : 0,
        ].join('|'),
    );
    rows.sort();
    for (const row of rows) lines.push('SOL|' + row);

    console.log(`${label}: ${results.length} 組解`);
}

// --- 1. cg-pet-calc 自己測試套件裡的三個真實案例 ---------------------------
function emitNamed(label, name, lvl, hp, mp, atk, def, agi, notorderpoint = 0) {
    const pet = PetDefaultData.filter((n) => n[1] === name)[0];
    if (!pet) throw new Error(`找不到寵物 ${name}`);
    const tiers = [pet[3], pet[4], pet[5], pet[6], pet[7]];
    const bprate = pet[8] == null ? 0.2 : parseFloat(pet[8]);
    emitCase(label, tiers, bprate, lvl, [hp, mp, atk, def, agi], notorderpoint);
}

emitNamed('紅色口臭鬼-lv1', '紅色口臭鬼', 1, 122, 102, 36, 33, 28);
emitNamed('聖誕水藍鼠-lv98', '聖誕水藍鼠', 98, 2308, 1327, 935, 328, 281);
emitNamed('純白液態史萊姆-lv59', '純白液態史萊姆', 59, 1815, 701, 280, 189, 140, 13);

// --- 2. 合成案例：由已知組態正算出能力，再反推 -----------------------------
// 涵蓋不同等級、加點分布、以及 cg-pet-calc 舊表查不到的高檔次區段。
const synth = [
    ['低檔次-lv1', [10, 8, 6, 4, 12], 1, [0, 0, 0, 0, 0], [2, 2, 2, 2, 2]],
    ['中檔次-lv20-無加點', [30, 25, 18, 20, 22], 20, [0, 0, 0, 0, 0], [3, 1, 2, 2, 2]],
    ['中檔次-lv30-有加點', [30, 25, 18, 20, 22], 30, [10, 5, 0, 3, 2], [2, 2, 2, 2, 2]],
    ['偏敏-lv25', [22, 14, 16, 40, 18], 25, [0, 0, 0, 8, 0], [1, 1, 1, 5, 2]],
    ['高檔次-lv40', [60, 55, 70, 48, 52], 40, [0, 0, 0, 0, 0], [2, 2, 2, 2, 2]],
    ['高檔次-lv60-有加點', [55, 62, 51, 58, 66], 60, [5, 5, 5, 5, 5], [0, 3, 3, 2, 2]],
    ['極高檔次-lv50', [90, 85, 100, 78, 95], 50, [0, 0, 0, 0, 0], [2, 2, 2, 2, 2]],
];

for (const [label, tiers, lvl, manual, random] of synth) {
    const bprate = 0.2;
    const g = new GrowRange(...tiers, bprate);
    const base = g.calcBPAtLevel(lvl, null).baseBP.toArray();
    const x = base.map((b, i) => b + bprate * random[i] + manual[i]);
    const s = new BP(...x).calcRealNum();
    const point = manual.reduce((a, b) => a + b, 0);
    emitCase(label, tiers, bprate, lvl, [s.hp, s.mp, s.attack, s.defend, s.agi], lvl - 1 - point);
}

writeFileSync(new URL('../vectors/guess.txt', import.meta.url), lines.join('\n') + '\n', 'utf8');
const cases = lines.filter((l) => l.startsWith('CASE')).length;
const sols = lines.filter((l) => l.startsWith('SOL')).length;
console.log(`\nwrote ${cases} cases / ${sols} solutions`);
