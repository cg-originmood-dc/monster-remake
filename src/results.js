// 推算結果與成長表的呈現。
//
// ## ⚠️ 更正紀錄：原程式結果欄一列 ＝ 一組**表觀檔次**
//
// 這個檔頭改過四次，前三次都寫錯，全部留著當紀錄：
//
// 1. 最早寫「原程式是把結果吐進一個 TMemo」—— **錯**，那個 TMemo 在「模擬」視窗上，
//    跟推算無關。
// 2. 接著改寫成「原程式不列候選解，列的是掉檔組合」—— **也錯**，而且錯得最久。
// 3. 再改成「原程式一列一組候選解」—— **還是錯**。這一次是只讀了印字串那段就下結論，
//    沒有往上追那張表是誰建的。它**有**去重，只是去重的鍵不是掉檔。
//
// 把建表那支也讀完之後，原程式結果欄的每一列是：
//
// ```
//  %2d檔 ＋ 五個對齊過的整數 ＋ [系N] ＋ %5.2f% ＋ [*]
// ```
//
// - 開頭那個是**總掉檔**（Σ圖鑑檔次 − Σ實際檔次）
// - 中間五個**不是掉檔**，是 **表觀檔次 − 圖鑑檔次**，其中
//   `表觀檔次 = 實際檔次 + 隨機檔`。所以這五個數 `a[i] = random[i] − lost[i]`，
//   **加起來恆等於 `10 − 總掉檔`**（隨機檔總和恆為 10）。原程式自己的提示字講的
//   就是這個量：「負數表示 100% 的掉檔，非負數的數值越高，掉檔概率越低」。
// - 最後是這一組的機率。
//
// ⭐ **一列的機率是「所有能拼出這組表觀檔次的（掉檔, 隨機檔）拆法」的權重總和。**
//    原程式對每一列跑一圈巢狀迴圈把那些拆法的權重累加進該列，最後除以全體 ×100
//    —— 所以列與列加起來剛好 100%，跟使用者截圖裡那三列 11.52 + 39.90 + 48.58 一致。
//    權重湊不出來（＝ 0）的列會被搬到表尾且**整列不印**。
//
// 所以這一塊吃的是引擎的 `distribution.apparent_combos`，**不是** `shown.candidates`
// —— 後者在應用層有 200 筆上限，拿它算全集的百分比會少一截。
//
// 現在的版面：**穩掉 → 計算結果（原程式那幾列）→ 掉檔組合 → 邊際分布**。
// 「掉檔組合」是**移植版自己的聚合**（鍵是掉檔而不是表觀檔次），已照實標明。
//
// ⚠️ 兩處刻意沒照抄，都是純排版的東西：
//  - 原程式把值 10 印成 ` a`（欄寬只有兩格，`10` 會擠掉對齊）。HTML 表格沒有這個
//    問題，照抄只會讓人看不懂，所以印 10。
//  - 原程式的列序就是它建表的順序，那個沒有逆出來；這裡照總掉檔遞增排。

import { useState } from 'preact/hooks';
import { html, Panel, Btn } from './ui.js';
import { modeName } from './panels.js';
import { ProbabilityQuery } from './probability.js';
import { MoreInfo } from './refine.js';

const AXIS = ['體', '力', '防', '敏', '魔'];

/**
 * 推算結果：逐軸邊際分布（原程式的答案）＋ 候選明細（附錄）。
 *
 * `onNarrow` 是「輸入更多資訊」篩完之後把掉檔範圍寫回側欄搜尋範圍的出口
 * —— 原程式也是這樣回頭改那一欄的（見 `refine.js`）。
 */
export const GuessResults = ({ resp, constants, onNarrow }) => {
    // hooks 不能擺在早退後面，所以這兩行一定要跑得到。
    const [refined, setRefined] = useState(null);
    // 篩選結果綁在**它篩的那一次推算**上 —— `resp` 一換，上一次的結果同一拍就失效。
    // 用 effect 去清會慢一拍，那一拍會拿舊候選畫在新推算的標題底下。
    const fresh = refined?.of === resp ? refined.out : null;

    if (!resp) return null;
    if (resp.total === 0) {
        return html`
            <${Panel} title="推算結果" className="result-panel">
                <p class="empty">
                    推不出任何解。
                    <br />
                    常見原因：能力值打錯、等級不對、或這隻的圖鑑檔次比實際低。
                    可以試試放寬比對模式，或確認「入手等級」有沒有填。
                </p>
            <//>
        `;
    }

    // 填過「輸入更多資訊」之後，底下每一塊講的都是**倖存的那批候選**
    // —— 原程式也是這樣（`setzjsx` 篩完就地重畫），不是另開一個結果區。
    const shown = fresh?.result ?? resp;
    const cut = fresh != null && fresh.result.total !== fresh.before;
    const d = shown.distribution;
    return html`
        <${Panel} title="推算結果" className="result-panel">
            <div class="result-head">
                <span class="chip">共 ${shown.total} 組解</span>
                <span class="chip">可配點數 ${shown.point}</span>
                <span class="chip">比對 ${modeName(shown.mode)}</span>
                ${d?.lost_total_range && html`<span class="chip">總掉檔 ${rangeText(d.lost_total_range)}</span>`}
                ${d?.fully_determined && html`<span class="chip good">唯一解</span>`}
                ${
                    cut &&
                    html`<span
                        class="chip cut"
                        title="百分比已對倖存的候選重新正規化；這裡的數字是它們在篩選前佔的比重"
                    >
                        ${`已篩掉 ${fresh.before - shown.total} 組（留下的原本佔 ${fresh.kept_percent.toFixed(2)}%）`}
                    </span>`
                }
            </div>

            <${MoreInfo} resp=${resp} constants=${constants} out=${fresh}
                onResult=${(out) => setRefined({ of: resp, out })} onNarrow=${onNarrow} />

            ${d && html`<${Marginals} d=${d} constants=${constants} />`}

            <details class="detail-block">
                <summary>
                    候選明細（${shown.candidates.length} 組${shown.truncated ? '，已截斷' : ''}）
                    <span class="sub-note">
                        同一批候選攤開來看；原程式的結果欄只印上面那三欄，加點這幾欄是移植版加的
                    </span>
                </summary>
                <div class="table-wrap">
                    <table class="grid">
                        <thead>
                            <tr>
                                <th class="pct">機率</th>
                                <th colspan="5">檔次</th>
                                <th colspan="6">掉檔</th>
                                <th colspan="5">加點</th>
                                <th colspan="5">隨機檔</th>
                            </tr>
                            <tr class="sub">
                                <th></th>
                                ${AXIS.map((a) => html`<th key=${'g' + a}>${a}</th>`)}
                                ${AXIS.map((a) => html`<th key=${'l' + a}>${a}</th>`)}
                                <th class="tot" title="這一組的總掉檔數">總</th>
                                ${AXIS.map((a) => html`<th key=${'m' + a}>${a}</th>`)}
                                ${AXIS.map((a) => html`<th key=${'r' + a}>${a}</th>`)}
                            </tr>
                        </thead>
                        <tbody>
                            ${shown.candidates.map(
                                (c, i) => html`
                                    <tr key=${i} class=${c.approximate ? 'approx' : ''}>
                                        <td
                                            class="pct"
                                            title=${c.approximate ? '容差比對命中，非精確解' : ''}
                                        >
                                            ${c.percent.toFixed(2)}%${c.approximate ? '＊' : ''}
                                        </td>
                                        ${c.grow.map((v, j) => html`<td key=${'g' + j}>${v}</td>`)}
                                        ${c.lost.map(
                                            (v, j) =>
                                                html`<td key=${'l' + j} class=${v ? 'lost' : 'zero'}>
                                                    ${v || '·'}
                                                </td>`,
                                        )}
                                        <td class="tot ${c.lost_bp ? 'lost' : 'zero'}">
                                            ${c.lost_bp || '·'}
                                        </td>
                                        ${c.manual.map(
                                            (v, j) =>
                                                html`<td key=${'m' + j} class=${v ? '' : 'zero'}>
                                                    ${v || '·'}
                                                </td>`,
                                        )}
                                        ${c.random.map(
                                            (v, j) => html`<td key=${'r' + j}>${v}</td>`,
                                        )}
                                    </tr>
                                `,
                            )}
                        </tbody>
                    </table>
                </div>
                ${
                    shown.truncated &&
                    html`<p class="hint">
                        只列前 ${shown.candidates.length} 組。上面的機率分布是對**全部
                        ${shown.total} 組**加權統計的，不受這個上限影響。
                    </p>`
                }
                ${
                    shown.candidates.some((c) => c.approximate) &&
                    html`<p class="hint">
                        ＊ 是靠放寬取整邊界才命中的（原程式獨有的機制，見 CLAUDE.md §3.4）。
                        ${
                            // 放寬是精確的**超集**，所以沒標星號的那些精確列舉也找得到。
                            // 只有在「全部都是星號」而且沒被截斷時，才敢說精確完全無解
                            // —— 截斷過的話看不到的那 N 組裡可能就有精確解。
                            !shown.truncated && shown.candidates.every((c) => c.approximate)
                                ? '這組觀測值精確列舉推不出任何解。'
                                : '沒標星號的那些精確列舉也找得到。'
                        }
                    </p>`
                }
            </details>

            ${
                // ⚠️ 這裡刻意傳**沒篩過的** `resp`：`api.probability` 掃的是引擎裡那份
                // 完整候選（見 `probability.js` 檔頭），跟「輸入更多資訊」篩出來的子集
                // 是兩回事。傳 `shown` 進去只會讓「依 N 組候選加權」那行對不上它算的東西。
                html`<${ProbabilityQuery} resp=${resp} constants=${constants} />`
            }
        <//>
    `;
};

/**
 * 逐軸的邊際機率 —— **這就是原程式的推算輸出**（`showdet`）。
 *
 * 原程式的規則是「某軸有一格 ≈100% 就代表那一軸已確定，五軸全確定就不顯示明細」。
 * 全確定時那五排長條圖每排都只有一根滿格的柱子，看了也是白看，
 * 所以照它的作法收成一行結論。
 *
 * ⚠️ **兩張表的地位不一樣。** 原程式的 `showdet` 只做掉檔那張 5×5；
 * 隨機檔那張是移植版自己算的（引擎本來就有 `random_marginal`，只是先前沒人畫）。
 * 所以「全確定就收起來」這條**只套在掉檔上** —— 掉檔定死了但隨機檔還有好幾種可能
 * 是很常見的（同一組檔次配不同隨機檔），那時候把隨機檔一起藏掉就是真的弄丟資訊，
 * 而且原程式也沒有「藏隨機檔」這個行為可以照抄。
 *
 * ⚠️ 「全確定就收起來」也**不套在「計算結果」那張表上**。原程式印那幾列的迴圈是
 * 無條件的，被 `showdet` 擋掉的只有底下的 5×5 明細；而且掉檔全確定時表觀檔次仍然
 * 可以有好幾組（同一組掉檔配不同隨機檔），藏起來就是弄丟原程式有印的東西。
 */
const Marginals = ({ d, constants }) => {
    const sure = (p) => 100 - p < constants.percent_epsilon;
    // 隨機檔沒有現成的 determined 欄位（DTO 只給了掉檔那組），
    // 用同一個門檻自己判 —— 門檻取自引擎，不要在這裡編一個近似值。
    const sureRandom = d.random_marginal.map((row) => row.findIndex(sure));

    const randomBlock = html`
        <${MargBlock}
            title="隨機檔機率"
            rows=${d.random_marginal}
            constants=${constants}
            sure=${sure}
            label=${(n) => `隨機檔 ${n}`}
            note=${(axis) => (sureRandom[axis] >= 0 ? `確定 ${sureRandom[axis]}` : '')}
        />
    `;

    const totalBlock = html`<${TotalLost} d=${d} sure=${sure} constants=${constants} />`;

    if (d.fully_determined) {
        const allRandom = sureRandom.every((n) => n >= 0);
        return html`
            <div class="marginals">
                <${ApparentRows} d=${d} />
                <div class="solved">
                    <h4>掉檔</h4>
                    <div class="solved-rows">
                        ${constants.axis_names.map(
                            (name, axis) => html`
                                <span class="solved-cell" key=${axis}>
                                    <span class="s-k">${name}</span>
                                    <span class="s-v">掉 ${d.determined_lost[axis]} 檔</span>
                                    ${
                                        allRandom &&
                                        html`<span class="s-r">隨機 ${sureRandom[axis]}</span>`
                                    }
                                </span>
                            `,
                        )}
                        ${
                            d.lost_total_range &&
                            html`<span class="solved-cell total">
                                <span class="s-k">總掉檔</span>
                                <span class="s-v">${rangeText(d.lost_total_range)}</span>
                            </span>`
                        }
                    </div>
                    <p class="hint">
                        五軸的掉檔量都只有一種可能${allRandom ? '，隨機檔也是' : ''} ——
                        ${allRandom ? '這組觀測值已經定死了。' : '剩下的不確定性都在隨機檔上。'}
                    </p>
                </div>
                ${!allRandom && randomBlock}
            </div>
        `;
    }

    return html`
        <div class="marginals">
            <${SureLoss} d=${d} />
            <${ApparentRows} d=${d} />
            <${LostCombos} d=${d} />
            ${totalBlock}
            <${MargBlock}
                title="掉檔機率"
                rows=${d.lost_marginal}
                constants=${constants}
                sure=${sure}
                label=${(n) => `掉 ${n} 檔`}
                note=${(axis) =>
                    d.determined_lost[axis] != null ? `確定掉 ${d.determined_lost[axis]} 檔` : ''}
            />
            ${randomBlock}
        </div>
    `;
};

/**
 * ⭐ **穩掉** —— 每一軸「一定至少掉了幾檔」。原程式主視窗結果欄的第一行就是它
 * （`穩掉：2體 4防 3魔`，掉 0 檔的軸不列）。
 *
 * ⚠️ 這**不是** `determined_lost`。那個只認「整條長條圖只剩一格」的軸，所以
 * 體力可能掉 2/3/4 檔時它一句話都不說 —— 但「至少掉 2 檔」明明就是確定的。
 * 使用者回報的正是這個落差：畫面只講「確定掉 4 檔（強度）」，沒講體力 2、魔法 3。
 *
 * 值一律由引擎的 `guaranteed_lost` 給，不要在這裡自己掃 `lost_marginal` 找第一個
 * 非零格 —— 判準（權重 > 0）是引擎那邊定的，兩邊各寫一次就會有一天不一樣。
 */
const SureLoss = ({ d }) => {
    const hit = d.guaranteed_lost.map((n, axis) => ({ n, axis })).filter((x) => x.n > 0);

    return html`
        <div class="sure-loss">
            <span class="sl-k">穩掉</span>
            ${
                hit.length === 0
                    ? html`<span class="sl-none">沒有哪一軸是一定會掉的</span>`
                    : hit.map(
                          ({ n, axis }) => html`
                              <span class="sl-v" key=${axis}>${n}<em>${AXIS[axis]}</em></span>
                          `,
                      )
            }
        </div>
    `;
};

/**
 * ⭐ **原程式的「計算結果」欄** —— 一列一組**表觀檔次**，欄位逐項照它印的東西。
 *
 * 原程式那一列是 `%2d檔 ` ＋ 五個對齊過的整數 ＋ 機率，五個整數是
 * **表觀檔次 − 圖鑑檔次**，不是掉檔：
 *
 * ```
 * 表觀檔次[i] = 實際檔次[i] + 隨機檔[i]
 * a[i]        = 表觀檔次[i] − 圖鑑檔次[i]  =  random[i] − lost[i]
 * ```
 *
 * 這個量的意義由原程式自己的提示字釘住：「這個數字叫表觀檔次…負數表示 100% 的
 * 掉檔，非負數的數值越高，掉檔概率越低」。負數 ＝ 連最好的隨機檔都補不回圖鑑檔次，
 * 那一軸鐵定掉了；非負數 ＝ 有機會沒掉，數字越大越安全。
 *
 * 恆等式（拿來驗欄位有沒有接錯）：`Σa = 10 − 總掉檔`，因為隨機檔總和恆為 10。
 * 原程式開頭那個 `檔` 也正是這樣算出來的（先加總再減 10，最後取負）。
 *
 * ⭐ **一列 ≠ 一組候選解。** 同一組表觀檔次可以由好幾種（掉檔, 隨機檔）拆法湊出來，
 * 原程式把那些拆法的權重**全部加在同一列上**，最後除以全體 ×100 —— 所以列與列
 * 加起來是 100%。這件事在引擎的 `apparent_combos` 算好了；這裡不能改吃
 * `shown.candidates`，那個有 200 筆上限，百分比會少一截。
 *
 * ⚠️ 值 10 原程式印成 ` a` —— 那是欄寬只有兩格的排版補丁，HTML 表格沒這個問題，
 * 所以這裡照實印 10。
 */
const ApparentRows = ({ d }) => {
    const rows = d.apparent_combos ?? [];
    if (rows.length === 0) return null;
    const peak = Math.max(...rows.map((c) => c.percent));

    return html`
        <div class="combo-block">
            <h4>計算結果<span class="sub-note">${rows.length} 組表觀檔次</span></h4>
            <div class="combo-wrap">
                <table class="grid combos">
                    <thead>
                        <tr>
                            <th class="tot" title="Σ圖鑑檔次 − Σ實際檔次">總</th>
                            ${AXIS.map(
                                (a) => html`<th
                                    key=${'a' + a}
                                    title="表觀檔次 − 圖鑑檔次（＝隨機檔 − 掉檔）。負數代表這一軸鐵定掉了，非負數越大越可能沒掉"
                                >
                                    ${a}
                                </th>`,
                            )}
                            <th class="pct">機率</th>
                        </tr>
                    </thead>
                    <tbody>
                        ${rows.map(
                            (c, i) => html`
                                <tr key=${i} class=${c.percent === peak ? 'peak' : ''}>
                                    <td class="tot">${c.total}</td>
                                    ${c.apparent.map(
                                        (v, j) => html`<td
                                            key=${j}
                                            class=${v < 0 ? 'lost' : v === 0 ? 'zero' : ''}
                                        >
                                            ${v}
                                        </td>`,
                                    )}
                                    <td class="pct">${c.percent.toFixed(2)}%</td>
                                </tr>
                            `,
                        )}
                    </tbody>
                </table>
            </div>
        </div>
    `;
};

/**
 * 相異的**掉檔組合**與各自的機率。
 *
 * ⚠️ **原程式沒有這一塊，是移植版自己的聚合。** 這裡本來寫「原程式結果欄那幾列」
 * —— 那句是錯的，原程式那幾列是上面的 `ApparentRows`：一列一組**表觀檔次**，
 * 五個數是 `隨機檔 − 掉檔`。這一塊改用**掉檔 5 元組**當鍵，是這邊自己加的；
 * 留著是因為它回答的是另一個問題（「到底哪幾軸掉了」），而那個問題原程式的
 * 結果欄答不了 —— 同一組掉檔會散在好幾列表觀檔次上。
 *
 * 逐軸邊際也做不到 —— 它把「哪一軸配哪一軸」丟掉了，所以看不出
 * 「掉 13 檔的時候是哪五個數字」。
 *
 * 排序：總掉檔遞增，同總掉檔時照元組遞增（引擎已經排好，這裡不再動）。
 * 數字是**正的掉檔量**，跟上面那張表的表觀檔次差不同號 —— 兩張表的第一欄
 * 都叫「總」而且**確實是同一個總掉檔**，差別在後面那五格是怎麼分解它的。
 */
const LostCombos = ({ d }) => {
    const combos = d.lost_combos ?? [];
    if (combos.length === 0) return null;
    const peak = Math.max(...combos.map((c) => c.percent));

    return html`
        <div class="combo-block">
            <h4>掉檔組合<span class="sub-note">${combos.length} 種</span></h4>
            <div class="combo-wrap">
                <table class="grid combos">
                    <thead>
                        <tr>
                            <th class="tot" title="這一組的總掉檔數">總</th>
                            ${AXIS.map((a) => html`<th key=${a}>${a}</th>`)}
                            <th class="pct">機率</th>
                        </tr>
                    </thead>
                    <tbody>
                        ${combos.map(
                            (c, i) => html`
                                <tr key=${i} class=${c.percent === peak ? 'peak' : ''}>
                                    <td class="tot">${c.total}</td>
                                    ${c.lost.map(
                                        (v, j) =>
                                            html`<td key=${j} class=${v ? 'lost' : 'zero'}>
                                                ${v || '·'}
                                            </td>`,
                                    )}
                                    <td class="pct">${c.percent.toFixed(2)}%</td>
                                </tr>
                            `,
                        )}
                    </tbody>
                </table>
            </div>
        </div>
    `;
};

/** `[lo, hi]` → `「3」` 或 `「2–7」`。 */
const rangeText = ([lo, hi]) => (lo === hi ? `${lo}` : `${lo}–${hi}`);

/**
 * 總掉檔的分布 —— 「這隻總共掉了幾檔」。
 *
 * ⚠️ **這個數字只能由引擎給**，前端算不出來，理由有兩層：
 *
 * 1. 掃 `candidates` 不行 —— 那個欄位有 200 筆上限（`truncated`），
 *    尾巴上的解看不到。
 * 2. 由 `lost_marginal` 逐軸推也不行 —— 逐軸邊際丟掉了軸之間的相關性，
 *    各軸最小值的和只是下界，未必真的有哪一組候選同時取到那些值。
 *    （`stats.rs::per_axis_marginals_cannot_recover_the_total_range` 釘住這件事。）
 *
 * 所以走 `distribution.lost_total_marginal`，那是對**全集**加權統計的。
 * 只畫有機率的那一段 —— 21 格裡通常只有三五格不是零。
 */
const TotalLost = ({ d, sure, constants }) => {
    const range = d.lost_total_range;
    if (!range) return null;
    const [lo, hi] = range;
    const slots = [];
    for (let n = lo; n <= hi; n++) slots.push(n);
    const peak = Math.max(...slots.map((n) => d.lost_total_marginal[n]), 1);

    return html`
        <div class="marg-block">
            <h4>
                總掉檔
                <span class="sub-note">
                    ${lo === hi ? `一定是 ${lo} 檔` : `${lo} 到 ${hi} 檔都有可能`}
                </span>
            </h4>
            <div class="marg-rows">
                <div class="marg-row">
                    <span class="marg-axis">合計</span>
                    ${slots.map((n) => {
                        const p = d.lost_total_marginal[n];
                        return html`
                            <span
                                key=${n}
                                class="marg-cell ${sure(p) ? 'sure' : ''} ${
                                    p < constants.percent_epsilon ? 'nil' : ''
                                }"
                                title="總共掉 ${n} 檔：${p.toFixed(2)}%"
                            >
                                <b>${n}</b>
                                <i style=${`height:${Math.max(1, (p / peak) * 28)}px`}></i>
                            </span>
                        `;
                    })}
                </div>
            </div>
        </div>
    `;
};

/**
 * 一張邊際分布表：每軸一列，列上每格是一根按機率高矮的柱子。
 *
 * 柱高用「該列最大值」正規化而不是固定 0..100 —— 隨機檔有 11 格，
 * 攤平之後常常每格都不到 20%，照絕對比例畫會五排都是矮到看不出差別的柱子。
 */
const MargBlock = ({ title, rows, constants, sure, label, note }) => html`
    <div class="marg-block">
        <h4>${title}</h4>
        <div class="marg-rows ${rows[0].length > 6 ? 'wide' : ''}">
            ${rows.map((row, axis) => {
                const peak = Math.max(...row, 1);
                return html`
                    <div class="marg-row" key=${axis}>
                        <span class="marg-axis">${constants.axis_names[axis]}</span>
                        ${row.map(
                            (p, n) => html`
                                <span
                                    key=${n}
                                    class="marg-cell ${sure(p) ? 'sure' : ''} ${
                                        p < constants.percent_epsilon ? 'nil' : ''
                                    }"
                                    title="${label(n)}：${p.toFixed(2)}%"
                                >
                                    <b>${n}</b>
                                    <i style=${`height:${Math.max(1, (p / peak) * 28)}px`}></i>
                                </span>
                            `,
                        )}
                        <span class="marg-note">${note(axis)}</span>
                    </div>
                `;
            })}
        </div>
    </div>
`;

/** 「模擬」頁的成長表。 */
export const SeriesTable = ({ rows, constants }) => {
    if (!rows || rows.length === 0) return null;
    return html`
        <${Panel} title="成長模擬" className="result-panel">
            <div class="table-wrap">
                <table class="grid">
                    <thead>
                        <tr>
                            <th>等級</th>
                            ${constants.stat_names.map((n) => html`<th key=${n}>${n}</th>`)}
                            <th colspan="5">加點</th>
                            <th>總 BP</th>
                        </tr>
                    </thead>
                    <tbody>
                        ${rows.map(
                            (r) => html`
                                <tr key=${r.lvl}>
                                    <td class="pct">${r.lvl}</td>
                                    <td>${r.stat.hp}</td>
                                    <td>${r.stat.mp}</td>
                                    <td>${r.stat.atk}</td>
                                    <td>${r.stat.def}</td>
                                    <td>${r.stat.agi}</td>
                                    <td>${r.stat.wis}</td>
                                    <td>${r.stat.res}</td>
                                    ${r.manual.map(
                                        (v, j) =>
                                            html`<td key=${j} class=${v ? '' : 'zero'}>
                                                ${v || '·'}
                                            </td>`,
                                    )}
                                    <td>${r.bp_sum.toFixed(2)}</td>
                                </tr>
                            `,
                        )}
                    </tbody>
                </table>
            </div>
        <//>
    `;
};

/**
 * 主視窗底下那排即時算出來的能力值 ＋ 逐軸 BP。
 *
 * ⭐ **逐軸 BP 那排是移植版自己加的**，原程式沒有 —— 它的「模擬」頁只有
 * 入手等級／當前等級／檔次／隨機檔四列，一格 BP 都沒印（截圖對過了）。
 *
 * 加它的理由不是照抄原程式，是**對帳**：先前只印 `總BP`，而總和對不回任何畫面；
 * 遊戲的「寵物狀態」視窗顯示的正是這五個數（`體力 8.4 力量 9.4 …`），
 * 所以逐軸那排是唯一能直接跟遊戲比對的東西。
 *
 * 小數位取 1 也是照**遊戲**的顯示 —— 這格的用途就是拿去對，跟得緊的才有用。
 * 能力值本身仍然是 `fix()` 取整後的整數，那條是公式，沒有動。
 */
export const StatStrip = ({ row, constants }) => {
    if (!row) return null;
    const s = row.stat;
    const vals = [s.hp, s.mp, s.atk, s.def, s.agi, s.wis, s.res];
    const bp = row.bp.map((v) => v.toFixed(1));
    return html`
        <div class="stat-strip">
            ${constants.stat_names.map(
                (n, i) => html`
                    <span class="stat-cell" key=${n}>
                        <span class="s-k">${n}</span>
                        <span class="s-v">${vals[i]}</span>
                    </span>
                `,
            )}
        </div>
        <div class="bp-strip">
            <span class="bp-k">BP</span>
            ${bp.map(
                (v, i) => html`
                    <span class="bp-cell" key=${i}>
                        <em>${AXIS[i]}</em>
                        ${v}
                    </span>
                `,
            )}
            <span class="bp-cell sum">
                <em>總</em>
                ${row.bp_sum.toFixed(2)}
            </span>
            <${Btn} title="複製這五個數（空白分隔）" onClick=${() => copy(bp.join(' '))}>複製<//>
        </div>
    `;
};

/** 剪貼簿在 http 或舊瀏覽器上沒有，失敗就安靜放過 —— 這顆按鈕不值得跳錯誤。 */
const copy = (text) => navigator.clipboard?.writeText(text).catch(() => {});
