// 機率查詢 —— 原程式側欄的機率統計（CLAUDE.md §3.5）。
//
// 問的是「我的寵在不在這個檔次範圍裡」而不是「哪一組最可能」：推算列出所有候選之後，
// 這裡把符合條件的那些依權重加起來，給一個百分比。原程式輸出兩個數 ——
// 五軸都落在範圍內的機率，以及選定幾軸的檔次合計達到門檻的機率。
//
// ⚠️ 這支一定要走後端。回傳的候選表被截成前 N 筆（`resp.truncated`），拿那些
// 自己加總會少算；後端留了完整候選與分布，見 lib.rs 的 `GuessCache`。

import { useEffect, useRef, useState } from 'preact/hooks';
import * as api from './api.js';
import { html, Row, Btn, NumField, AxisFields } from './ui.js';

/** 跟 Rust 端 `stats::PERCENT_EPSILON` 同一個常數：原程式用它判定「已確定」。 */
const PERCENT_EPSILON = 0.01;

const AXIS_SHORT = ['體', '力', '防', '敏', '魔'];

export const ProbabilityQuery = ({ resp, constants }) => {
    const seed = resp.candidates[0].grow;
    const [lo, setLo] = useState(seed);
    const [hi, setHi] = useState(seed);
    const [axes, setAxes] = useState([false, false, false, false, false]);
    const [threshold, setThreshold] = useState(0);
    const [out, setOut] = useState(null);
    const [err, setErr] = useState(null);
    const seq = useRef(0);

    // 換了一次推算就是換了一批候選（後端的快取也跟著換），舊的結果不能留。
    useEffect(() => {
        const g = resp.candidates[0].grow;
        setLo(g);
        setHi(g);
        setAxes([false, false, false, false, false]);
        setThreshold(0);
        setOut(null);
        setErr(null);
    }, [resp]);

    /**
     * 勾軸的同時把門檻挪到「最可能那組在這幾軸上的合計」。
     *
     * 不這樣做的話，勾完軸看到的第一個數幾乎一定是 0%（門檻是別組軸的量級），
     * 使用者得先自己算一次才知道要填多少。
     */
    const toggleAxis = (i) => {
        const next = axes.map((v, j) => (j === i ? !v : v));
        setAxes(next);
        setThreshold(seed.reduce((a, v, j) => (next[j] ? a + v : a), 0));
    };

    // 查詢只是對候選加權掃一遍，很便宜，所以邊改邊算；抖動抑制擋掉連打。
    const key = JSON.stringify([lo, hi, axes, threshold, resp.total]);
    useEffect(() => {
        const mine = ++seq.current;
        const t = setTimeout(async () => {
            const picked = axes.map((on, i) => (on ? i : -1)).filter((i) => i >= 0);
            try {
                const r = await api.probability({ lo, hi, axes: picked, threshold });
                if (mine === seq.current) {
                    setOut(r);
                    setErr(null);
                }
            } catch (e) {
                if (mine === seq.current) {
                    setOut(null);
                    setErr(String(e));
                }
            }
        }, 150);
        return () => clearTimeout(t);
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [key]);

    return html`
        <div class="prob">
            <h4>機率查詢</h4>

            <${Row} label="檔次下限">
                <${AxisFields}
                    names=${constants.axis_names}
                    values=${lo}
                    min=${0}
                    max=${constants.max_tier}
                    onChange=${setLo}
                    extras=${html`
                        <${Btn} title="放寬成全範圍" onClick=${() => setLo([0, 0, 0, 0, 0])}>C<//>
                    `}
                />
            <//>

            <${Row} label="檔次上限">
                <${AxisFields}
                    names=${null}
                    values=${hi}
                    min=${0}
                    max=${constants.max_tier}
                    onChange=${setHi}
                    extras=${html`
                        <${Btn}
                            title="放寬成全範圍"
                            onClick=${() => setHi(new Array(5).fill(constants.max_tier))}
                        >
                            C
                        <//>
                    `}
                />
            <//>

            <${Row} label="合計軸">
                <div class="btn-row">
                    ${AXIS_SHORT.map(
                        (label, i) => html`
                            <${Btn}
                                key=${i}
                                active=${axes[i]}
                                title=${constants.axis_names[i]}
                                onClick=${() => toggleAxis(i)}
                            >
                                ${label}
                            <//>
                        `,
                    )}
                </div>
                <span class="dash">合計 ≥</span>
                <${NumField} value=${threshold} min=${0} size="md" onChange=${setThreshold} />
            <//>

            ${err && html`<p class="hint bad-note">${err}</p>`}

            ${
                out &&
                html`
                <div class="prob-out">
                    <${Readout} label="落在檔次範圍內" value=${out.in_range} />
                    ${
                        out.axis_sum != null
                            ? html`<${Readout}
                              label=${`${axes
                                  .map((on, i) => (on ? AXIS_SHORT[i] : ''))
                                  .join('')} 合計 ≥ ${threshold}`}
                              value=${out.axis_sum}
                          />`
                            : html`<span class="prob-note">選幾個軸就會多算一個「合計 ≥ 門檻」的機率</span>`
                    }
                    <span class="prob-note">依 ${out.candidates} 組候選加權${
                        resp.truncated ? `（表上只列出前 ${resp.candidates.length} 組）` : ''
                    }</span>
                </div>
            `
            }
        </div>
    `;
};

/** 一個百分比讀數。原程式用 `%.3f`，接近 0／100 時改顯示文字。 */
const Readout = ({ label, value }) => {
    const sure = 100 - value < PERCENT_EPSILON;
    const nil = value < PERCENT_EPSILON;
    return html`
        <span class="prob-cell ${sure ? 'sure' : ''} ${nil ? 'nil' : ''}">
            <span class="p-k">${label}</span>
            <span class="p-v">${sure ? '確定' : nil ? '不可能' : `${value.toFixed(3)}%`}</span>
        </span>
    `;
};
