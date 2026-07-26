// 左側直立分頁展開出來的三塊側欄。
//
// 欄位與版面照原程式的實機截圖（CLAUDE.md §4.3）。原程式把這些
// 拆在窄版與寬版兩個表單裡，這裡合成同一塊面板的三個分頁。
//
// 側欄**沒有標題列** —— 原程式那兩個表單就是一塊光板子，是哪一頁看亮起來的
// 直立分頁就知道了。

import { useState } from 'preact/hooks';
import { html, Panel, Row, Btn, BtnRow, ToggleRow, NumField, SpinField, AxisFields } from './ui.js';

/**
 * 推算的搜尋範圍預設值 —— 原程式那兩欄的出廠值。
 *
 * 原程式寫成兩串 `上限;下限` 的字串：
 * `檔次範圍 4 4 4 4 4;0 0 0 0 0`、`隨機檔範圍 0 0 0 0 0;10 10 10 10 10`。
 * 這裡拆成四個陣列，語意一樣。
 */
export const DEFAULT_RANGES = {
    loss_min: [0, 0, 0, 0, 0],
    loss_max: [4, 4, 4, 4, 4],
    random_min: [0, 0, 0, 0, 0],
    random_max: [10, 10, 10, 10, 10],
};

/** 側欄分頁 —— 對應原程式左緣那排直立標籤。 */
export const SIDE_TABS = [
    { key: 'analyze', label: '分析' },
    { key: 'expand', label: '擴展' },
    { key: 'simulate', label: '模擬' },
];

const AXIS_SHORT = [
    { key: 0, label: '體' },
    { key: 1, label: '力' },
    { key: 2, label: '防' },
    { key: 3, label: '敏' },
    { key: 4, label: '魔' },
];

const PLAN_OPTIONS = [
    { key: 'free', label: '不限制', title: '加點怎麼配都可以' },
    { key: 'fixed', label: '指定加點', title: '五軸各釘死一個數' },
    { key: 'mixed', label: '混加', title: '只有勾起來的軸可以加點' },
];

// ── 分析 ────────────────────────────────────────────────────────────────────

/**
 * 推算用的條件：入手等級、能力無效、加點方式。
 *
 * 「入手等級 N」的意思是這隻在 N 級被抓到，1→N 的等級點是系統配的、不是玩家配的
 * —— 也就是後端的 `not_order_point = N - 1`。這是野寵推算跟家寵最大的差別。
 */
export const AnalyzePanel = ({ s, set, constants, catchParsed }) => {
    const wild = s.calcMode === 'wild';
    // 填了但認不齊五項時把欄位轉紅 —— 跟主視窗的「當前能力」同一套規矩。
    const catchBad = s.useCatchStat && !!s.catchStatText && !catchParsed?.ok;
    // 「無／體／力／防／敏／魔」的加點是模式本身定死的，後端會直接忽略這裡填的東西
    // （dto.rs 的 `takes_manual_plan`）。所以整組鎖起來，也不要留著上一個模式的欄位。
    const planFixed = s.planEditable && s.plan === 'fixed';
    const planMixed = s.planEditable && s.plan === 'mixed';

    return html`
        <${Panel} className="side-panel">
            <${Row} label="入手等級">
                <${SpinField}
                    value=${s.catchLvl}
                    min=${1}
                    max=${s.lvl}
                    onChange=${(v) => set({ catchLvl: v })}
                />
                <${Btn}
                    active=${s.useCatchStat}
                    onClick=${() => set({ useCatchStat: !s.useCatchStat })}
                >
                    ${s.useCatchStat ? '使用入手能力' : '不用入手能力'}
                <//>
            <//>

            <${Row} label=${s.useCatchStat ? '入手能力' : '能力無效'}>
                <input
                    type="text"
                    class="field wide ${catchBad ? 'bad' : ''}"
                    disabled=${!s.useCatchStat}
                    value=${s.catchStatText}
                    placeholder="入手時的血魔攻防敏"
                    title=${
                        s.useCatchStat && catchParsed?.matched
                            ? `認出 ${catchParsed.matched} 項`
                            : ''
                    }
                    onInput=${(e) => set({ catchStatText: e.currentTarget.value })}
                />
                ${catchBad && html`<span class="parse-note bad">只認出 ${catchParsed.matched} 項</span>`}
            <//>

            ${
                /* 入手當下玩家一點也還沒加，所以這串能力只由「實際檔次 × 隨機檔」決定。
                  當前能力那條式子有加點可以吸收誤差，這條沒有 —— 填得出來的話
                  通常能把隨機檔直接壓到唯一。 */ ''
            }
            ${
                s.useCatchStat &&
                html`<p class="hint">
                入手當下還沒加點，所以這串只由實際檔次與隨機檔決定 —— 填對了通常能把隨機檔壓到唯一。
            </p>`
            }

            <${Row} label="加點方式">
                <${BtnRow}
                    options=${PLAN_OPTIONS}
                    value=${s.planEditable ? s.plan : null}
                    disabled=${!s.planEditable}
                    onChange=${(plan) => set({ plan })}
                />
                ${
                    !s.planEditable &&
                    html`<span class="hint">「${modeLabel(constants, s.calcMode)}」已經把加點定死了</span>`
                }
            <//>

            ${
                planFixed &&
                html`
                <${Row} label="指定加點">
                    <${AxisFields}
                        names=${constants.axis_names}
                        values=${s.fixedPoints}
                        onChange=${(fixedPoints) => set({ fixedPoints })}
                        extras=${html`<span class="rest">餘 ${restPoints(s)}</span>`}
                    />
                <//>
            `
            }

            ${
                planMixed &&
                html`
                <${Row} label="混加種類">
                    <${ToggleRow}
                        options=${AXIS_SHORT}
                        value=${s.mixedAxes}
                        onChange=${(mixedAxes) => set({ mixedAxes })}
                    />
                <//>
                <${Row} label="混加下界">
                    <${AxisFields}
                        names=${constants.axis_names}
                        values=${s.mixedLo}
                        onChange=${(mixedLo) => set({ mixedLo })}
                    />
                <//>
                <${Row} label="混加上界">
                    <${AxisFields}
                        names=${null}
                        values=${s.mixedHi}
                        onChange=${(mixedHi) => set({ mixedHi })}
                    />
                <//>
            `
            }

            ${
                wild &&
                html`<p class="hint wild">
                野寵的 1→${s.catchLvl} 級是系統隨機配的，這 ${Math.max(0, s.catchLvl - 1)}
                點不算玩家加點。
            </p>`
            }

            <${Row} label="比對方式">
                <${BtnRow}
                    options=${MATCH_MODES}
                    value=${s.matchMode}
                    onChange=${(matchMode) => set({ matchMode })}
                />
            <//>
            <p class="hint">${MATCH_MODE_HINT[s.matchMode]}</p>

            <${SearchRanges}
                value=${s.ranges}
                onChange=${(ranges) => set({ ranges })}
                constants=${constants}
            />
        <//>
    `;
};

/**
 * 推算的比對方式。**「魔觀」排第一而且是預設** —— 這個專案要的是跟原程式一樣。
 *
 * 「魔觀」是原程式的規則：只在該軸檔次落在成長係數表五週期的不規則處
 * （`檔次 % 5 ∈ {0, 1}`）才放寬取整邊界，容差 `= clamp((等級−入手等級)/200, 0.01, 0.1)`。
 * 「精確」是 cg-pet-calc 的作法，留著是因為它是對拍的基準。
 * 「寬鬆」則是移植版早期照錯誤理解做的全軸放寬，比原程式鬆，留著當最後手段。
 */
const MATCH_MODES = [
    { key: 'observer', label: '魔觀', title: '原程式的規則：只修不規則檔次的取整邊界（預設）' },
    { key: 'exact', label: '精確', title: '取整後完全相等（cg-pet-calc 的作法）' },
    { key: 'tolerant', label: '寬鬆', title: '全軸一律放寬 ±0.1（移植版的延伸，比原程式鬆）' },
    { key: 'auto', label: '自動', title: '由緊到鬆：精確 → 魔觀 → 寬鬆，第一個有解的就用' },
];

const MATCH_MODE_HINT = {
    observer: '原程式的規則：只修「檔次 ÷ 5 餘 0 或 1」那幾軸的取整邊界 —— 誤差正是從那裡來的。',
    exact: '完全不放寬（cg-pet-calc 的作法）。野寵等級一高常常直接無解。',
    tolerant: '五軸全開，可能多出原程式不會給的解。魔觀推不出來時的最後手段。',
    auto: '推不出解時自動放寬，用到哪一段會顯示在下面。',
};

/**
 * 推算的搜尋範圍 —— 原程式 `TForm2.panelct` 那一頁。
 *
 * 原程式把它做成獨立的一頁，兩欄各填一整串 `上限;下限`。移植版折進「分析」裡
 * 收起來放：側欄的直立分頁是圖檔 sprite，原程式只畫了三個，加第四個得憑空造圖；
 * 而這幾個數字跟同一頁的「入手能力／加點方式」本來就是同一類東西
 * —— 都是拿來縮小推算解空間的約束。
 *
 * 預設值就是先前寫死的搜尋寬度，所以不碰它的話行為跟以前一模一樣
 * （`wire_check.rs::omitting_the_search_ranges_matches_spelling_out_the_defaults`）。
 */
const SearchRanges = ({ value, onChange, constants }) => {
    const [open, setOpen] = useState(false);
    const names = constants.axis_names;
    const touched = RANGE_FIELDS.some((f) => value[f].some((v, i) => v !== DEFAULT_RANGES[f][i]));

    // 隨機檔那五格的總和恆為 10，所以下界合計 > 10 或上界合計 < 10 就是自相矛盾。
    // 後端也擋（`check_ranges`），但要按下「計算」才看得到 —— 這裡當場講。
    const rlo = sum(value.random_min);
    const rhi = sum(value.random_max);
    const pool = constants.random_pool;
    const contradiction =
        rlo > pool
            ? `下界合計 ${rlo} 已經超過 ${pool}`
            : rhi < pool
              ? `上界合計 ${rhi} 湊不到 ${pool}`
              : null;

    // 軸名只掛在每一對的第一列（`_min`），跟上面的「混加下界／上界」同一個規矩。
    const field = (key, label, max) => html`
        <${Row} label=${label}>
            <${AxisFields}
                names=${key.endsWith('_min') ? names : null}
                values=${value[key]}
                min=${0}
                max=${max}
                onChange=${(next) => onChange({ ...value, [key]: next })}
            />
        <//>
    `;

    return html`
        <${Row} label="搜尋範圍">
            <${Btn} active=${open} onClick=${() => setOpen(!open)}>${open ? '收起' : '展開'}<//>
            ${touched && html`<span class="rest">已改過預設</span>`}
        <//>
        ${
            open &&
            html`
            <p class="hint warn">
                ${'提示：如果你不知道自己在做什麼，請不要改變這裡的設定。'}
                ${'限定條件是拿來加快驗證結果的，填錯了會把正確答案濾掉。'}
            </p>
            ${field('loss_min', '掉檔下限', constants.max_loss_bound)}
            ${field('loss_max', '掉檔上限', constants.max_loss_bound)}
            ${field('random_min', '隨機檔下限', pool)}
            ${field('random_max', '隨機檔上限', pool)}
            <${Row} label="">
                <${Btn} disabled=${!touched} onClick=${() => onChange({ ...DEFAULT_RANGES })}>
                    還原
                <//>
                ${contradiction && html`<span class="parse-note bad">${contradiction}</span>`}
            <//>
        `
        }
    `;
};

const RANGE_FIELDS = ['loss_min', 'loss_max', 'random_min', 'random_max'];

const sum = (a) => a.reduce((x, y) => x + y, 0);

function modeLabel(constants, key) {
    return constants.calc_modes.find((m) => m.key === key)?.label ?? key;
}

/** 指定加點還剩幾點沒配掉 —— 負數代表配超過了。 */
function restPoints(s) {
    const pool = Math.max(0, s.lvl - 1 - Math.max(0, s.catchLvl - 1));
    return pool - s.fixedPoints.reduce((a, b) => a + b, 0);
}

// ── 擴展 ────────────────────────────────────────────────────────────────────

const GROW_MODE = [
    { key: 'none', label: '不加' },
    { key: 'hp', label: '體力' },
    { key: 'atk', label: '力量' },
    { key: 'def', label: '強度' },
    { key: 'agi', label: '速度' },
    { key: 'mp', label: '魔法' },
];

/**
 * 「暴點加點」＝ 從**下一級起**改把等級點加到這一軸；`next`（下次）＝ 不換軸。
 *
 * ⚠️ 這是重新定義過的語意。原程式這個下拉只寫不讀，對計算完全沒有影響，
 * 真正的意圖已經無從還原 —— 詳見 `dto.rs` 的 `ForwardReq::burst`。
 * 與其擺一個按了沒反應的控制項，不如給它一個明確、只用到既有正算路徑的意思。
 */
const BURST = [
    { key: 'next', label: '下次' },
    { key: 'hp', label: '體力' },
    { key: 'atk', label: '力量' },
    { key: 'def', label: '強度' },
    { key: 'agi', label: '速度' },
    { key: 'mp', label: '魔法' },
];

const BURST_HINT =
    '暴點加點：從下一級起改加這一軸（「下次」＝不換軸）。' +
    '搭配左邊的「加點」就能看「一直加 A、之後改加 B」的成長表。';

/**
 * 手動把檔次／隨機檔填進去、直接看能力 —— 原程式的「展開」頁。
 *
 * 這一頁不做推算，是反過來的：已知檔次時拿來驗證，或試各種隨機檔的結果。
 */
export const ExpandPanel = ({ s, set, constants, coef }) => html`
    <${Panel} className="side-panel wide">
        <${Row} label="入手等級">
            <${SpinField}
                value=${s.catchLvl}
                min=${1}
                max=${s.lvl}
                onChange=${(v) => set({ catchLvl: v })}
            />
            <span class="coef">能力係數: ${coef.toFixed(3)}</span>
        <//>

        <${Row} label="當前等級">
            <${SpinField} value=${s.lvl} onChange=${(v) => set({ lvl: v })} />
            <select
                class="field pick"
                value=${s.growMode}
                onChange=${(e) => set({ growMode: e.currentTarget.value })}
            >
                ${GROW_MODE.map((o) => html`<option key=${o.key} value=${o.key}>加點：${o.label}</option>`)}
            </select>
            <select
                class="field pick ${s.burst === 'next' ? '' : 'on'}"
                title=${BURST_HINT}
                value=${s.burst}
                onChange=${(e) => set({ burst: e.currentTarget.value })}
            >
                ${BURST.map((o) => html`<option key=${o.key} value=${o.key}>暴點：${o.label}</option>`)}
            </select>
        <//>

        <${Row} label="檔次">
            <${AxisFields}
                names=${constants.axis_names}
                values=${s.simGrow}
                min=${0}
                max=${constants.max_tier}
                onChange=${(simGrow) => set({ simGrow })}
                extras=${html`
                    <${Btn} title="清成 0" onClick=${() => set({ simGrow: [0, 0, 0, 0, 0] })}>C<//>
                    <${Btn}
                        title="從主視窗的最高檔次帶過來（＝一檔都沒掉）"
                        onClick=${() => set({ simGrow: [...s.grow] })}
                    >
                        ↑
                    <//>
                `}
            />
        <//>
        ${
            lostFrom(s).some((n) => n !== 0) &&
            html`<${Row} label="掉檔">
            <span class="rest ${lostFrom(s).some((n) => n < 0) ? 'bad' : ''}">
                ${lostFrom(s)
                    .map(
                        (n, i) =>
                            `${constants.axis_names[i]}${n > 0 ? `−${n}` : n < 0 ? `+${-n}` : '0'}`,
                    )
                    .join('　')}
                ${lostFrom(s).some((n) => n < 0) ? '（實際比圖鑑高，檢查一下）' : ''}
            </span>
        <//>`
        }

        <${Row} label="隨機檔">
            <${AxisFields}
                names=${null}
                values=${s.random}
                min=${0}
                max=${constants.random_pool}
                onChange=${(random) => set({ random })}
                extras=${html`
                    <${Btn} title="清成 0" onClick=${() => set({ random: [0, 0, 0, 0, 0] })}>C<//>
                    <${Btn}
                        title="回到平均值 ${constants.default_random.join(' ')}"
                        onClick=${() => set({ random: [...constants.default_random] })}
                    >
                        復位
                    <//>
                `}
            />
            <span class="rest ${randomSum(s) === constants.random_pool ? '' : 'bad'}">
                合計 ${randomSum(s)} / ${constants.random_pool}
            </span>
        <//>
    <//>
`;

const randomSum = (s) => s.random.reduce((a, b) => a + b, 0);

/** 實際檔次比圖鑑低多少 —— 負數代表填反了。 */
const lostFrom = (s) => s.grow.map((g, i) => g - s.simGrow[i]);

// ── 模擬 ────────────────────────────────────────────────────────────────────

/** 一段等級區間的成長表 —— 原程式的成長模擬視窗。 */
export const SimulatePanel = ({ s, set, onRun, busy }) => html`
    <${Panel} className="side-panel">
        <${Row} label="等級區間">
            <${NumField}
                value=${s.simFrom}
                min=${1}
                max=${255}
                onChange=${(simFrom) => set({ simFrom })}
            />
            <span class="dash">→</span>
            <${NumField}
                value=${s.simTo}
                min=${1}
                max=${255}
                onChange=${(simTo) => set({ simTo })}
            />
        <//>
        <${Row} label="">
            <${Btn} wide disabled=${busy} onClick=${onRun}>${busy ? '計算中…' : '產生成長表'}<//>
        <//>
        <p class="hint">
            用「擴展」頁的檔次與隨機檔正算，不做推算。加點依主視窗的運算模式逐級套用。
        </p>
    <//>
`;
