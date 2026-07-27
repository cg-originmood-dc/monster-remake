// 推算後的再篩選 —— 原程式的「輸入更多資訊」。
//
// ## 這是原程式的第二段
//
// 原程式推算完不會就結束：它把候選表留著，回過頭來問你「還知道什麼嗎」，
// 然後**在同一批候選上再掃一遍**。每動一格就重掃一次，某一欄在所有倖存候選裡
// 只剩一個值時，那格就不再問了；全部問完就收工。另有一顆「跳過」＝ 這題我不知道。
//
// ⭐ **關鍵是「什麼時候給約束」。** 側欄那些（入手等級／入手能力／加點方式／
// 搜尋範圍）都是**算之前**的條件，改一個就得整組重推；這一塊是**算之後**的，
// 掃的是已經建好的候選表，所以邊填邊看是即時的。
//
// ## ⭐ 這一版是照原程式的介面改的，先前那版是照誤解畫的
//
// 先前這裡畫 12 格「7 欄能力 ＋ 5 欄檔次」，還得在旁邊解釋生命／魔力／攻擊／
// 防禦／敏捷那五格為什麼不會自動變灰。**那五格原程式根本沒有。**
// 它那 12 個標籤的文字是編在執行檔裡的常數，解出來是
// `精神 回復 ｜ 體力 力量 強度 速度 魔法 ｜ 體力 力量 強度 速度 魔法` ——
// 兩欄能力、五欄 BP、五欄檔次（哪一組是哪個由比對方式分辨，說明在
// `petcore::dto::RefineReq`）。血魔攻防敏是推算的**輸入**，問了也篩不掉東西。
//
// 另外三件事也照抄了：
//
// | 原程式的行為                       | 這裡                                       |
// | ---------------------------------- | ------------------------------------------ |
// | 已確定的欄位連同標籤**隱藏**       | 不再畫那一格（先前是變灰，那是看錯了）     |
// | 篩到 0 組 → 該格的標籤變成「選錯」 | 同（先前是底下一行紅字）                   |
// | 一開始就只有一個值的欄位不會被問   | 同（拿第一次未篩的結果當基準）             |
//
// 「使用者碰過的格子永遠留著」也是原程式的：它在每次 OnChange 都把該欄標成
// 「已作答」，之後就不再隱藏 —— 這樣打錯字才有得改。

import { useEffect, useRef, useState } from 'preact/hooks';
import * as api from './api.js';
import { html, Row, Btn } from './ui.js';

const BLANK7 = [null, null, null, null, null, null, null];
const BLANK5 = [null, null, null, null, null];

/**
 * 原程式那 12 個框，順序與標籤逐字照抄。
 *
 * `kind` 決定送到哪個欄位、怎麼解析；`slot` 是該欄位裡的索引。
 * 能力只有精神（5）與回復（6）——原程式沒問前五格。
 */
const columns = (constants) => [
    { key: 'wis', kind: 'stat', slot: 5, label: constants.stat_names[5], group: '能力' },
    { key: 'res', kind: 'stat', slot: 6, label: constants.stat_names[6], group: '能力' },
    ...constants.axis_names.map((label, i) => ({
        key: `bp${i}`,
        kind: 'bp',
        slot: i,
        label,
        group: 'BP',
    })),
    ...constants.axis_names.map((label, i) => ({
        key: `grow${i}`,
        kind: 'grow',
        slot: i,
        label,
        group: '檔次',
        max: constants.max_tier,
    })),
];

const GROUPS = [
    ['能力', '遊戲看得到，推算沒用過的兩項'],
    ['BP', '寵物狀態視窗那一排'],
    ['檔次', '知道實際檔次的話'],
];

export const MoreInfo = ({ resp, constants, out, onResult, onNarrow }) => {
    const [stat, setStat] = useState(BLANK7);
    const [bp, setBp] = useState(BLANK5);
    const [grow, setGrow] = useState(BLANK5);
    // 使用者碰過的欄位（原程式的「已作答」旗標）—— 碰過就不再隱藏。
    const [touched, setTouched] = useState(() => new Set());
    // 最後動的那一格。只有它會被印上「選錯」，跟原程式一樣。
    const [last, setLast] = useState(null);
    const [skipped, setSkipped] = useState(false);
    const [err, setErr] = useState(null);
    // 這是第幾次推算。**不能拿 `resp.total` 當代號** —— 兩次推算剛好一樣多組解時
    // 底下那個 key 不會變，篩選就不會重跑，那批新候選也就永遠沒人去問「哪幾欄已確定」。
    const [gen, setGen] = useState(0);
    // 「這一欄值得問嗎」的基準：這次推算**還沒篩之前**哪些欄位是浮動的。
    // 原程式是在建問題時就決定的（只有多於一個值的欄位才會被建出來），
    // 一旦定下來就不會因為後來篩窄了而多冒出新問題。
    const baseline = useRef(null);
    const seq = useRef(0);
    // 上一次寫回側欄的掉檔範圍，用來擋掉重複的寫入（見底下那個 effect）。
    const pushed = useRef(null);

    // 換一次推算就是換一批候選（後端的快取也跟著換），舊的填答不能留。
    useEffect(() => {
        setStat(BLANK7);
        setBp(BLANK5);
        setGrow(BLANK5);
        setTouched(new Set());
        setLast(null);
        setSkipped(false);
        setErr(null);
        baseline.current = null;
        pushed.current = null;
        setGen((g) => g + 1);
    }, [resp]);

    // 篩選只是掃一遍候選，很便宜，所以邊改邊算；抖動抑制擋掉連打。
    // 掛載那一拍會排一次 gen=0 的查詢，但緊接著的重繪會把它 clearTimeout 掉，
    // 所以實際只送出 gen=1 那一次。
    const key = JSON.stringify([gen, stat, bp, grow]);
    useEffect(() => {
        const mine = ++seq.current;
        const t = setTimeout(async () => {
            try {
                const r = await api.refine({ stat, bp, grow });
                if (mine !== seq.current) return;
                // 第一次（什麼都沒填）的結果就是「哪幾欄值得問」的基準。
                baseline.current ??= r.settled;
                onResult(r);
                setErr(null);
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

    // ⭐ 原程式篩完會把倖存候選的逐軸掉檔範圍寫回「搜尋範圍」那兩欄
    //（`上限;下限`，見 CLAUDE.md §4.3）—— 下次按計算就在收窄過的空間裡重推。
    // 值由引擎掃全部倖存候選給（`narrowed_loss`），不是從邊際分布反推的。
    //
    // ⚠️ **只在使用者真的填過格子之後才寫**。原程式那段掛在輸入框的 OnChange 上，
    // 沒動過就不會執行；移植版第一次的空篩選是自己排的（用來問出「哪幾欄值得問」），
    // 那不是使用者的動作，跟著寫回去就會變成「按一下計算，側欄的搜尋範圍就被改掉」。
    const narrow = touched.size > 0 ? out?.narrowed_loss : null;
    useEffect(() => {
        if (!narrow || !onNarrow) return;
        const bounds = {
            loss_min: narrow.map(([lo]) => lo),
            loss_max: narrow.map(([, hi]) => hi),
        };
        const stamp = JSON.stringify(bounds);
        if (pushed.current === stamp) return;
        pushed.current = stamp;
        onNarrow(bounds);
    }, [narrow, onNarrow]);

    const clear = () => {
        setStat(BLANK7);
        setBp(BLANK5);
        setGrow(BLANK5);
    };

    // 「跳過」＝ 不再追問，用目前的結果收工。原程式在這一步會檢查篩選結果是不是
    // 空的，是的話把整批候選放回去 —— 照做，不然按下去畫面會停在「0 組解」。
    const skip = () => {
        if (out && out.result.total === 0) clear();
        setSkipped(true);
    };

    const settled = out?.settled;
    const empty = out != null && out.result.total === 0;
    const cols = columns(constants);
    const valueOf = (c) => ({ stat, bp, grow })[c.kind][c.slot];
    const settledOf = (c) => settled?.[c.kind]?.[c.slot];

    // 一開始就唯一的欄位不列（原程式連控制項都不會顯示）；
    // 之後變成唯一的就藏起來，除非使用者自己碰過那一格。
    // 篩到空的時候 `settled` 整份是 null，所以每一格都會自己露回來 —— 才改得動。
    const asked = (c) => baseline.current == null || baseline.current[c.kind][c.slot] == null;
    const visible = cols.filter((c) => asked(c) && (touched.has(c.key) || settledOf(c) == null));
    const done = !empty && cols.filter(asked).every((c) => settledOf(c) != null);

    if (skipped || (done && baseline.current != null)) {
        return html`<div class="more-info skipped">
            <span class="mi-k">輸入更多資訊</span>
            <span class="mi-note">${skipped ? '已跳過' : '問完了 —— 每一欄都只剩一個值'}</span>
            ${skipped && html`<${Btn} onClick=${() => setSkipped(false)}>再問一次<//>`}
        </div>`;
    }

    const change = (c, v) => {
        setTouched((s) => new Set(s).add(c.key));
        setLast(c.key);
        const put = (arr) => arr.map((o, j) => (j === c.slot ? v : o));
        if (c.kind === 'stat') setStat(put);
        else if (c.kind === 'bp') setBp(put);
        else setGrow(put);
    };

    return html`
        <div class="more-info">
            <h4>
                輸入更多資訊
                <span class="sub-note">知道多少填多少，留空 ＝ 不限。不重算，只把不合的候選砍掉</span>
            </h4>

            ${GROUPS.map(([group, hint]) => {
                const mine = visible.filter((c) => c.group === group);
                if (mine.length === 0) return null;
                return html`
                    <${Row} label=${group} key=${group}>
                        <div class="axis-fields">
                            <div class="axis-boxes">
                                ${mine.map(
                                    (c) => html`
                                        <input
                                            key=${c.key}
                                            type="text"
                                            inputMode=${c.kind === 'bp' ? 'decimal' : 'numeric'}
                                            class="field num sm"
                                            title=${hint}
                                            value=${valueOf(c) == null ? '' : String(valueOf(c))}
                                            onInput=${(e) => change(c, parse(e.currentTarget, c))}
                                        />
                                    `,
                                )}
                            </div>
                            <div class="axis-names">
                                ${mine.map(
                                    (c) => html`
                                        <span key=${c.key} class=${empty && last === c.key ? 'wrong' : ''}>
                                            ${empty && last === c.key ? '選錯' : c.label}
                                        </span>
                                    `,
                                )}
                            </div>
                        </div>
                    <//>
                `;
            })}

            <${Row} label="">
                <${Btn} title="清掉填過的格子" disabled=${touched.size === 0} onClick=${clear}>
                    清除
                <//>
                <${Btn} title="不再追問，用目前的結果收工" onClick=${skip}>跳過<//>
                ${
                    narrow &&
                    html`<span class="mi-narrow" title="原程式篩完也是這樣回頭改搜尋範圍的">
                        ${`掉檔範圍已寫回搜尋範圍：${narrow.map(([lo, hi]) => (lo === hi ? lo : `${lo}–${hi}`)).join(' ')}`}
                    </span>`
                }
            <//>

            ${err && html`<p class="hint bad-note">${err}</p>`}
        </div>
    `;
};

/**
 * 把輸入框的字轉成數字，留空 ＝ `null`。
 *
 * **不能用 `ui.js` 的 `NumField`** —— 它把空字串當成 `min ?? 0`（那是刻意的，
 * 它的呼叫端沒有「沒填」這個狀態）。這裡「留空」是真的有意義：留空 ＝ 這欄不限，
 * 跟填 0 是兩回事。
 *
 * BP 是小數（遊戲的寵物狀態視窗印一位小數），另外兩種是整數。
 */
function parse(el, col) {
    const raw =
        col.kind === 'bp'
            ? el.value.trim().replace(/[^\d.]/g, '')
            : el.value.trim().replace(/[^\d]/g, '');
    if (raw === '' || raw === '.') return null;
    const n = Number(raw);
    if (!Number.isFinite(n)) return null;
    return col.max == null ? n : Math.min(n, col.max);
}
