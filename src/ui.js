// 共用的介面零件。
//
// 原程式整個介面是 owner-drawn（DFM 裡只有背景圖跟空 TPanel，中文字全烘進點陣圖），
// 所以這裡**不**復刻繪圖碼 —— 見 CLAUDE.md §4（該節開頭那段）。但外框、分頁、
// 加減鈕這幾樣是純圖案（沒有文字要重排），就直接用從原程式資源抽出來的原圖，
// 放在 src/assets/ui/。其餘（按鈕、欄位、標籤）用 CSS 照截圖的取樣色重畫。

import { h } from 'preact';
import htm from 'htm';

export const html = htm.bind(h);

/**
 * 主視窗外框 —— 背景就是原程式的 `RT_BITMAP_6936`（281×173，右緣那捲軸也在圖裡）。
 *
 * 標題「噬生・魔物觀測者」是**烘在圖上的**，所以這裡不再畫一次標題列；
 * 內容貼在圖的凹槽上，凹槽位置以百分比寫在 CSS（原圖是 x20..253 / y27..160）。
 *
 * 標題列右上那兩顆珠子：青綠的「−」**也是烘在圖裡的**（x238..250 / y7..17），
 * 所以 `.shell-orb.min` 只是蓋一塊透明熱區上去；紅色的「?」原程式是執行期畫的，
 * 這裡就照截圖取樣的紅重畫一顆，尺寸與「−」對齊。
 */
export const Shell = ({ onHelp, onMinimize, children }) => html`
    <section class="shell">
        ${
            onHelp &&
            html`<button type="button" class="shell-orb help" title="說明" onClick=${onHelp}>
            ?
        </button>`
        }
        ${
            onMinimize &&
            html`<button
            type="button"
            class="shell-orb min"
            title="縮到工作列"
            onClick=${onMinimize}
        ></button>`
        }
        <div class="shell-body">${children}</div>
    </section>
`;

/** 一塊米色面板，可選的標題列與右上角小圖示。邊框是照原圖的五層描邊重畫的。 */
export const Panel = ({ title, icons, className = '', children }) => html`
    <section class="panel ${className}">
        ${
            title &&
            html`<header class="panel-title">
            <span>${title}</span>
            ${icons && html`<span class="panel-icons">${icons}</span>`}
        </header>`
        }
        <div class="panel-body">${children}</div>
    </section>
`;

/** 一列「標籤 ＋ 內容」。標籤欄寬固定，讓每一列的欄位左緣對齊。 */
export const Row = ({ label, children, className = '' }) => html`
    <div class="row ${className}">
        <span class="row-label">${label}</span>
        <div class="row-body">${children}</div>
    </div>
`;

export const Btn = ({ active, wide, tone, disabled, title, onClick, children }) => html`
    <button
        type="button"
        class="btn ${active ? 'active' : ''} ${wide ? 'wide' : ''} ${tone ? `tone-${tone}` : ''}"
        disabled=${disabled}
        title=${title}
        onClick=${onClick}
    >
        ${children}
    </button>
`;

/**
 * 一整排單選按鈕 —— 主視窗的「運算模式」與側欄的「加點方式」都是這個形狀。
 * `options` 是 `[{key, label, title?}]`。
 */
export const BtnRow = ({ options, value, onChange, disabled, className = '' }) => html`
    <div class="btn-row ${className}">
        ${options.map(
            (o) => html`
                <${Btn}
                    key=${o.key}
                    active=${o.key === value}
                    disabled=${disabled}
                    title=${o.title}
                    onClick=${() => onChange(o.key)}
                >
                    ${o.label}
                <//>
            `,
        )}
    </div>
`;

/** 可複選的按鈕排 —— 混加的「混加種類」用這個。`value` 是布林陣列。 */
export const ToggleRow = ({ options, value, onChange, className = '' }) => html`
    <div class="btn-row ${className}">
        ${options.map(
            (o, i) => html`
                <${Btn}
                    key=${o.key}
                    active=${!!value[i]}
                    onClick=${() => onChange(value.map((v, j) => (j === i ? !v : v)))}
                >
                    ${o.label}
                <//>
            `,
        )}
    </div>
`;

export const TextField = ({ value, onInput, placeholder, className = '', ...rest }) => html`
    <input
        type="text"
        class="field ${className}"
        value=${value}
        placeholder=${placeholder}
        onInput=${(e) => onInput(e.currentTarget.value)}
        ...${rest}
    />
`;

/**
 * 數字欄位。
 *
 * 刻意用 `type="text"` 而不是 `number`：數字輸入框在中途狀態（空字串、只有負號）
 * 會回傳空值，把 state 洗成 NaN，游標也會跳掉。這裡自己守著 —— 打不出數字就
 * 維持原值不動，使用者可以放心把整格清空再重打。
 */
export const NumField = ({ value, onChange, min, max, size = 'sm', className = '', title }) => html`
    <input
        type="text"
        inputMode="numeric"
        class="field num ${size} ${className}"
        title=${title}
        value=${String(value)}
        onInput=${(e) => {
            const raw = e.currentTarget.value.trim();
            if (raw === '') return onChange(min ?? 0);
            const n = Number(raw.replace(/[^\d-]/g, ''));
            if (Number.isFinite(n)) onChange(clamp(n, min, max));
        }}
    />
`;

/** 原程式那對 [−][+] 小方鈕。圖案是原圖（綠減／金加），所以按鈕本身不放文字。 */
export const Stepper = ({ onStep }) => html`
    <span class="stepper">
        <button type="button" class="step minus" title="−1" onClick=${() => onStep(-1)}></button>
        <button type="button" class="step plus" title="＋1" onClick=${() => onStep(1)}></button>
    </span>
`;

/** 一個數字 ＋ 一對加減鈕，側欄的「入手等級／當前等級」都長這樣。 */
export const SpinField = ({ value, onChange, min = 1, max = 255 }) => html`
    <span class="spin">
        <${NumField} value=${value} onChange=${onChange} min=${min} max=${max} />
        <${Stepper} onStep=${(d) => onChange(clamp(value + d, min, max))} />
    </span>
`;

/**
 * 五軸一排的小方格（檔次／隨機檔／指定加點都用它），下面帶一排軸名。
 * `values` 是 `[體力,力量,強度,速度,魔法]`。
 *
 * `className="trough"` 是主視窗那一版：原程式的「最高檔次」是**一條**長凹槽，
 * 所以五格併進同一個框裡、軸名改掛 tooltip，才不會多長出一排字把版面撐高。
 */
export const AxisFields = ({
    names,
    values,
    onChange,
    min = 0,
    max = 999,
    extras,
    className = '',
}) => html`
    <div class="axis-fields ${className}">
        <div class="axis-boxes">
            ${values.map(
                (v, i) => html`
                    <${NumField}
                        key=${i}
                        value=${v}
                        min=${min}
                        max=${max}
                        title=${names ? names[i] : undefined}
                        onChange=${(n) => onChange(values.map((o, j) => (j === i ? n : o)))}
                    />
                `,
            )}
            ${extras}
        </div>
        ${
            names &&
            !className.includes('trough') &&
            html`<div class="axis-names">${names.map((n) => html`<span>${n}</span>`)}</div>`
        }
    </div>
`;

export function clamp(n, min, max) {
    if (min != null && n < min) return min;
    if (max != null && n > max) return max;
    return n;
}
