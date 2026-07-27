// 推算後的再篩選 —— 原程式的「輸入更多資訊」。
//
// ## 這是原程式的第二段
//
// 原程式推算完不會就結束：它把候選表留著，回過頭來問你「還知道什麼嗎」，
// 然後**在同一批候選上再掃一遍**。12 個框（7 欄能力 ＋ 5 欄檔次，留空 ＝ 不限），
// 每動一格就重掃一次；某一欄在所有倖存候選裡只剩一個值時，那格就被停用
// （沒必要再問），12 欄全確定就收工。另有一顆「跳過」＝ 這題我不知道。
//
// ⭐ **關鍵是「什麼時候給約束」。** 側欄那些（入手等級／入手能力／加點方式／
// 搜尋範圍）都是**算之前**的條件，改一個就得整組重推；這一塊是**算之後**的，
// 掃的是已經建好的候選表，所以邊填邊看是即時的。
//
// ## 為什麼這幾格問得出新東西
//
// 推算的輸入只有五項能力（`TargetDto` 沒有精神／回復），所以**精神與回復在
// 候選之間是會變的** —— 那兩格是真的在給新資訊，5 欄檔次同理。
// 另外五格（生命 魔力 攻擊 防禦 敏捷）已經被查詢本身釘死，填了也篩不掉東西，
// 但照樣留著：原程式就是 7 格，而且它們會在第一次篩選後自己變成「已確定」。

import { useEffect, useRef, useState } from 'preact/hooks';
import * as api from './api.js';
import { html, Row, Btn } from './ui.js';
import { modeName } from './panels.js';

const BLANK7 = [null, null, null, null, null, null, null];
const BLANK5 = [null, null, null, null, null];

const isBlank = (a) => a.every((v) => v == null);

export const MoreInfo = ({ resp, constants, out, onResult }) => {
    const [stat, setStat] = useState(BLANK7);
    const [grow, setGrow] = useState(BLANK5);
    const [skipped, setSkipped] = useState(false);
    const [err, setErr] = useState(null);
    // 這是第幾次推算。**不能拿 `resp.total` 當代號** —— 兩次推算剛好一樣多組解時
    // 底下那個 key 不會變，篩選就不會重跑，那批新候選也就永遠沒人去問「哪幾欄已確定」。
    const [gen, setGen] = useState(0);
    const seq = useRef(0);

    // 換一次推算就是換一批候選（後端的快取也跟著換），舊的填答不能留。
    useEffect(() => {
        setStat(BLANK7);
        setGrow(BLANK5);
        setSkipped(false);
        setErr(null);
        setGen((g) => g + 1);
    }, [resp]);

    // 篩選只是掃一遍候選，很便宜，所以邊改邊算；抖動抑制擋掉連打。
    // 掛載那一拍會排一次 gen=0 的查詢，但緊接著的重繪會把它 clearTimeout 掉，
    // 所以實際只送出 gen=1 那一次。
    const key = JSON.stringify([gen, stat, grow]);
    useEffect(() => {
        const mine = ++seq.current;
        const t = setTimeout(async () => {
            try {
                const r = await api.refine({ stat, grow });
                if (mine === seq.current) {
                    onResult(r);
                    setErr(null);
                }
            } catch (e) {
                if (mine === seq.current) {
                    onResult(null);
                    setErr(String(e));
                }
            }
        }, 150);
        return () => clearTimeout(t);
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [key]);

    const clear = () => {
        setStat(BLANK7);
        setGrow(BLANK5);
    };

    // 「跳過」＝ 不再追問，用目前的結果收工。原程式在這一步會檢查篩選結果是不是
    // 空的，是的話退回全部候選 —— 照做，不然按下去畫面會停在「0 組解」。
    const skip = () => {
        if (out && out.result.total === 0) clear();
        setSkipped(true);
    };

    const settled = out?.settled;
    const done = settled?.all;

    // ⚠️ 血魔攻防敏那五格**照理說一進來就該是灰的** —— 推算的輸入就是它們，
    // 候選全都該算回同一組數字。沒變灰只有一個原因：`Observer`／`Tolerant`
    // 放行的候選是靠偏移量才命中的，而 `Candidate.stat` 存的是**沒加偏移**那份
    // BP 反算出來的能力，所以差得了 1。
    //
    // 原程式不會這樣：它的容差加在**檔次**上，能力值比對一律精確（CLAUDE.md §3.4）。
    // 我們把偏移接到 BP 上是推論不是實證（理由寫在 `petcalc::guess` 的模組說明），
    // 這一格就是那個推論露出來的地方 —— **不藏**。把同一個數字再填一次會看到解變少，
    // 沒有說明就是在騙人。
    const loose = settled ? [0, 1, 2, 3, 4].filter((i) => settled.stat[i] == null) : [];

    if (skipped) {
        return html`<div class="more-info skipped">
            <span class="mi-k">輸入更多資訊</span>
            <span class="mi-note">已跳過</span>
            <${Btn} onClick=${() => setSkipped(false)}>再問一次<//>
        </div>`;
    }

    return html`
        <div class="more-info">
            <h4>
                輸入更多資訊
                <span class="sub-note">
                    ${
                        done
                            ? '12 欄都定下來了 —— 沒有別的可以問了'
                            : '知道多少填多少，留空 ＝ 不限。不重算，只是把不合的候選砍掉'
                    }
                </span>
            </h4>

            <${Row} label="能力">
                <${Cells}
                    names=${constants.stat_names}
                    values=${stat}
                    settled=${settled?.stat}
                    onChange=${setStat}
                />
            <//>

            <${Row} label="檔次">
                <${Cells}
                    names=${constants.axis_names}
                    values=${grow}
                    settled=${settled?.grow}
                    max=${constants.max_tier}
                    onChange=${setGrow}
                />
                <${Btn} title="清掉全部 12 格" disabled=${isBlank(stat) && isBlank(grow)} onClick=${clear}>
                    清除
                <//>
                <${Btn} title="不再追問，用目前的結果收工" onClick=${skip}>跳過<//>
            <//>

            ${
                loose.length > 0 &&
                html`<p class="hint">
                    ${`${loose.map((i) => constants.stat_names[i]).join('／')} 沒有自動變灰：那幾格有候選是靠「${modeName(resp.mode)}」的容差才命中的，反算回來會差 1。照上面填過的數字再填一次 ＝ 只留精確吻合的那些。這一排真正問得出新東西的是精神與回復。`}
                </p>`
            }
            ${err && html`<p class="hint bad-note">${err}</p>`}
            ${
                out &&
                out.result.total === 0 &&
                html`<p class="hint bad-note">
                    這組條件把候選全砍光了 —— 有一格填錯了，或是這隻的推算本來就不含這個值。
                </p>`
            }
        </div>
    `;
};

/**
 * 一排格子。
 *
 * **不能用 `ui.js` 的 `NumField`** —— 它把空字串當成 `min ?? 0`（那是刻意的，
 * 它的呼叫端沒有「沒填」這個狀態）。這裡「留空」是真的有意義：留空 ＝ 這欄不限，
 * 跟填 0 是兩回事，所以自己守一個允許空值的版本。
 *
 * **已確定又沒被填過的欄位停用** —— 原程式就是把那格 `Enabled` 設成 false
 * （所有倖存候選在那一欄都同一個值，再問也問不出東西）。值本身照樣印出來，
 * 那是答案不是雜訊，所以 CSS 沒有沿用預設的 disabled 淡化。
 *
 * 使用者**自己填過**的那格不鎖：原程式是一問一答的單行道，這裡是隨時能回頭改的
 * 表單，把自己填的值鎖起來只會讓打錯字沒救。
 */
const Cells = ({ names, values, settled, max = 9999, onChange }) => html`
    <div class="axis-fields">
        <div class="axis-boxes">
            ${values.map((v, i) => {
                const fixed = v == null && settled?.[i] != null;
                return html`<input
                    key=${i}
                    type="text"
                    inputMode="numeric"
                    class="field num sm"
                    disabled=${fixed}
                    title=${fixed ? `${names[i]}：已確定` : names[i]}
                    value=${fixed ? String(settled[i]) : v == null ? '' : String(v)}
                    onInput=${(e) => {
                        const raw = e.currentTarget.value.trim().replace(/[^\d]/g, '');
                        const n = raw === '' ? null : Number(raw);
                        onChange(
                            values.map((o, j) =>
                                j !== i
                                    ? o
                                    : n == null || !Number.isFinite(n)
                                      ? null
                                      : Math.min(n, max),
                            ),
                        );
                    }}
                />`;
            })}
        </div>
        <div class="axis-names">${names.map((n) => html`<span key=${n}>${n}</span>`)}</div>
    </div>
`;
