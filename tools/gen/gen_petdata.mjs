// 把 cg-pet-calc 的 PetDefaultData 轉成給 Rust `include_str!` 用的 TSV。
//
// 為什麼不直接在 Rust 端 parse 那個 .mjs：那是 JS 原始碼，不是資料檔。
// 轉成 TSV 之後 Rust 端只要 split('\t')，也方便 diff。
//
// PetDefaultData 一筆的排版：
//   [編號, 名稱, 種族, 體力, 力量, 強度, 速度, 魔法, 能力倍率, 技能格?]
// 檔次五軸的順序是 [HP, ATK, DEF, AGI, MP]，與 spetdata.ini 相同。
//
// ⚠️ 上游其實有兩種列形狀：
//   完整列（272 筆，長度 10）編號／種族／技能格都有，數值是 number
//   精簡列（483 筆，長度  9）編號與種族是 null、沒有技能格，五項檔次是**字串**
// 精簡列是後來補進去的社群資料。這裡照原樣輸出（缺的欄位留空字串），
// 由 Rust 端決定怎麼補 —— 別在這裡編假編號出來。
//
// 用法： node tools/gen/gen_petdata.mjs

import { PetDefaultData } from 'cg-pet-calc';
import { writeFileSync } from 'node:fs';

const COLS = ['id', 'name', 'race', 'hp', 'atk', 'def', 'agi', 'mp', 'bprate', 'skills'];
const lines = [`# ${COLS.join('\t')}`];

let skipped = 0;
for (const p of PetDefaultData) {
    const [id, name, race, hp, atk, def, agi, mp, bprate, skills] = p;
    // 表裡有佔位的空洞（沒有名字），Rust 端不需要
    if (!name) {
        skipped++;
        continue;
    }
    const rate = bprate == null ? 0.2 : parseFloat(bprate);
    // null/undefined -> 空欄位＝「這筆資料沒有」，跟 0 區分開來
    const opt = (v) => (v == null ? '' : v);
    lines.push([opt(id), name, opt(race), hp, atk, def, agi, mp, rate, opt(skills)].join('\t'));
}

writeFileSync(
    new URL('../../crates/petdata/data/builtin_pets.tsv', import.meta.url),
    lines.join('\n') + '\n',
    'utf8',
);
console.log(`wrote ${lines.length - 1} pets (skipped ${skipped} nameless)`);
