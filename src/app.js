// 主視窗。
//
// 版面照原程式的實機截圖（CLAUDE.md §4.1）：
//
//   寵物資料  [🔍] [寵物名字 ............]        能力倍率
//   最高檔次  [ 27 16 25 15 37 ..........]        [ 20 ]
//   當前等級  [ 1 ] [−][+]            [筒] [保存] [計算]
//   當前能力  [ 格式:血魔攻防敏(精神回復) ......]
//   運算模式  [智][野][無][體][力][防][敏][魔]
//                 料理果然還是有個最美味的時間啊
//
// ⚠️ 一排的順序是 **[直立分頁][側欄][主視窗]** —— 側欄是往**左**開的，
// 分頁永遠貼在最左邊那塊的左緣、被它壓住一半。原程式側欄關與側欄開的截圖
// 都是這個樣子，不要照直覺擺到右邊去。

import { useCallback, useEffect, useMemo, useRef, useState } from 'preact/hooks';
import * as api from './api.js';
import { parseStats, formatStats } from './parse.js';
import { html, Shell, Row, Btn, BtnRow, NumField, Stepper, AxisFields, clamp } from './ui.js';
import { PetPicker } from './petpicker.js';
import { AnalyzePanel, ExpandPanel, SimulatePanel, SIDE_TABS, DEFAULT_RANGES } from './panels.js';
import { GuessResults, SeriesTable, StatStrip } from './results.js';
import { CatalogWindow } from './catalog.js';

const STATUS_IDLE = '料理果然還是有個最美味的時間啊';

const initialState = (constants) => ({
    petName: '',
    /** 主視窗的「最高檔次」＝ 圖鑑上限，推算時當作掉檔前的起點。 */
    grow: [0, 0, 0, 0, 0],
    /**
     * 「擴展」頁的「檔次」＝ **實際**檔次（已經掉過檔的）。
     * 這兩個是不同的東西：推算是從最高檔次往下找實際檔次，
     * 而正算是拿實際檔次直接算能力。選寵物時兩邊一起填，之後各走各的。
     */
    simGrow: [0, 0, 0, 0, 0],
    /** 能力倍率在介面上是 ×100 的整數（原程式恆為 20 ＝ 0.20）。 */
    bprate100: Math.round(constants.default_bprate * 100),
    lvl: 1,
    statText: '',

    calcMode: 'smart',
    plan: 'free',
    fixedPoints: [0, 0, 0, 0, 0],
    mixedAxes: [true, true, true, true, true],
    mixedLo: [0, 0, 0, 0, 0],
    mixedHi: [99, 99, 99, 99, 99],

    catchLvl: 1,
    useCatchStat: false,
    catchStatText: '',

    /** 推算的搜尋範圍（原程式的「檔次範圍／隨機檔範圍」）。預設＝引擎的預設寬度。 */
    ranges: { ...DEFAULT_RANGES },

    /**
     * 比對方式。預設 `observer` ＝ **原程式的規則**（見 `panels.js` 的 `MATCH_MODES`）——
     * 這個專案要的是跟原程式一樣，不是跟 cg-pet-calc 一樣。
     */
    matchMode: 'observer',

    random: [...constants.default_random],
    growMode: 'none',
    burst: 'next',

    simFrom: 1,
    simTo: 50,
});

export function App({ boot }) {
    const { constants } = boot;
    const [catalog, setCatalog] = useState(boot.catalog);
    const [s, setS] = useState(() => initialState(constants));
    const [tab, setTab] = useState('analyze');
    // 自製寵物存檔讀壞時要講出來 —— 使用者的資料還在檔案裡，這時候去按「保存」
    // 就會把它整個蓋掉，所以一開場就得看到。
    const [status, setStatus] = useState(boot.custom_error ?? STATUS_IDLE);
    const [busy, setBusy] = useState(false);
    const [guessResp, setGuessResp] = useState(null);
    const [seriesRows, setSeriesRows] = useState(null);
    const [showCatalog, setShowCatalog] = useState(false);

    const set = useCallback((patch) => setS((prev) => ({ ...prev, ...patch })), []);

    const parsed = useMemo(() => parseStats(s.statText), [s.statText]);
    // 「入手能力」跟「當前能力」用同一個寬鬆解析器 —— 使用者兩格都是用貼的。
    const parsedCatch = useMemo(() => parseStats(s.catchStatText), [s.catchStatText]);
    const growDto = useMemo(() => toGrowDto(s.grow, s.bprate100), [s.grow, s.bprate100]);
    const simGrowDto = useMemo(() => toGrowDto(s.simGrow, s.bprate100), [s.simGrow, s.bprate100]);

    // 「擴展」頁的正算預覽：實際檔次＋隨機檔＋加點 → 能力。改什麼都即時重算，
    // 但每次敲鍵都跑一趟 IPC 太吵，所以壓一下。
    const preview = useLivePreview({ s, growDto: simGrowDto, active: tab === 'expand' });

    const modeInfo = constants.calc_modes.find((m) => m.key === s.calcMode);
    const planEditable = !!modeInfo?.takes_manual_plan;

    const pickPet = (p) => {
        set({
            petName: p.name,
            grow: [...p.grow],
            // 還沒推算之前，實際檔次的最好猜測就是圖鑑值（＝一檔都沒掉）
            simGrow: [...p.grow],
            bprate100: Math.round(p.bprate * 100),
        });
        setStatus(`${p.name}　${p.race_name}　檔次 ${p.grow.join(' ')}`);
    };

    const runGuess = async () => {
        if (!parsed.ok) {
            setStatus('「當前能力」至少要有血魔攻防敏五個數字');
            return;
        }
        // 勾了「使用入手能力」卻填不出五項時要擋下來，不能默默不送 ——
        // 使用者以為條件生效了，看到的解卻是沒帶條件的那批。
        const catchTarget = buildCatchTarget(s, parsedCatch);
        if (s.useCatchStat && !catchTarget) {
            setStatus('「入手能力」也要有血魔攻防敏五個數字，不然就先關掉「使用入手能力」');
            return;
        }
        setBusy(true);
        setStatus('推算中…');
        try {
            const resp = await api.guess(
                buildGuessReq(s, growDto, parsed, planEditable, catchTarget),
            );
            setGuessResp(resp);
            setSeriesRows(null);
            setStatus(describeGuess(resp, !!catchTarget));
        } catch (e) {
            setGuessResp(null);
            setStatus(String(e));
        } finally {
            setBusy(false);
        }
    };

    const runSeries = async () => {
        setBusy(true);
        try {
            const rows = await api.series({
                ...forwardBase(s, simGrowDto),
                from: s.simFrom,
                to: s.simTo,
            });
            setSeriesRows(rows);
            setGuessResp(null);
            setStatus(`成長表 ${s.simFrom} → ${s.simTo} 共 ${rows.length} 級`);
        } catch (e) {
            setStatus(String(e));
        } finally {
            setBusy(false);
        }
    };

    const growCoef = useMemo(() => {
        // 「能力係數」＝ 這隻每升一級的成長率，取五軸平均（原程式顯示一個數）
        const rates = s.simGrow.map((t) =>
            t >= 0 && t <= constants.max_tier ? constants.full_rates[t] : 0,
        );
        return rates.reduce((a, b) => a + b, 0) / rates.length;
    }, [s.simGrow, constants]);

    const sideProps = { s: { ...s, planEditable }, set, constants };

    return html`
        <div class="app">
            <div class="deck ${tab ? 'with-panel' : ''}">
                <nav class="side-tabs ${tab ? 'with-panel' : ''}">
                    ${SIDE_TABS.map(
                        (t) => html`
                            <button
                                key=${t.key}
                                type="button"
                                class="side-tab tab-${t.key} ${tab === t.key ? 'on' : ''}"
                                title=${t.label}
                                onClick=${() => setTab(tab === t.key ? null : t.key)}
                            ></button>
                        `,
                    )}
                </nav>

                ${
                    /* 側欄的位子**永遠留著**（`.side-slot` 是固定寬的），關起來只是空著。
                      不留的話開合會把主視窗左右推 —— 桌面程式往左長不影響別人，
                      網頁上那是整塊版面在跳。面板在槽裡靠右貼著主視窗，
                      所以最寬的「擴展」頁展開時也不會頂到主視窗。 */ ''
                }
                <div class="side-slot">
                    ${
                        tab === 'analyze' &&
                        html`<${AnalyzePanel} ...${sideProps} catchParsed=${parsedCatch} />`
                    }
                    ${tab === 'expand' && html`<${ExpandPanel} ...${sideProps} coef=${growCoef} />`}
                    ${
                        tab === 'simulate' &&
                        html`<${SimulatePanel} ...${sideProps} onRun=${runSeries} busy=${busy} />`
                    }
                </div>

                ${
                    /* 原程式標題列右上有 [?] 跟 [−] 兩顆。網頁版沒有工作列可以縮，
                      所以不傳 onMinimize —— Shell 會乾脆不畫那顆，
                      而不是留一顆按了沒反應的鈕。 */ ''
                }
                <${Shell} onHelp=${() => setStatus(HELP)}>
                    <${Row} label="寵物資料">
                        <button
                            type="button"
                            class="lens"
                            title="開啟寵物檔案"
                            onClick=${() => setShowCatalog(true)}
                        ></button>
                        ${
                            /* 原程式這一列是純標題列：「寵物資料」🔍「寵物名字」…「能力倍率」，
                              一個凹槽都沒有。所以名字欄用 bare —— 靜著看是一行字，
                              滑過去才變輸入框。 */ ''
                        }
                        <${PetPicker}
                            className="bare"
                            pets=${catalog.pets}
                            value=${s.petName}
                            onPick=${pickPet}
                        />
                        <span class="col-cap">能力倍率</span>
                    <//>

                    <${Row} label="最高檔次">
                        <${AxisFields}
                            className="trough"
                            names=${constants.axis_names}
                            values=${s.grow}
                            min=${0}
                            max=${constants.max_tier}
                            onChange=${(grow) => set({ grow })}
                        />
                        <${NumField}
                            value=${s.bprate100}
                            min=${1}
                            max=${100}
                            size="md"
                            onChange=${(bprate100) => set({ bprate100 })}
                        />
                    <//>

                    <${Row} label="當前等級">
                        <${NumField}
                            value=${s.lvl}
                            min=${1}
                            max=${255}
                            onChange=${(lvl) => set({ lvl })}
                        />
                        <${Stepper} onStep=${(d) => set({ lvl: clamp(s.lvl + d, 1, 255) })} />
                        <span class="spacer"></span>
                        <${Btn}
                            title="原程式此鈕功能未確認；這裡拿來開寵物檔案"
                            onClick=${() => setShowCatalog(true)}
                        >
                            筒
                        <//>
                        <${Btn}
                            title="把目前的能力字串複製回剪貼簿"
                            onClick=${() => save(s, preview, setStatus)}
                        >
                            保存
                        <//>
                        <${Btn} disabled=${busy} onClick=${runGuess}>
                            ${busy ? '計算中' : '計算'}
                        <//>
                    <//>

                    <${Row} label="當前能力">
                        <input
                            type="text"
                            class="field wide ${s.statText && !parsed.ok ? 'bad' : ''}"
                            value=${s.statText}
                            placeholder="格式:血魔攻防敏(精神回復)"
                            title=${s.statText ? `認出 ${parsed.matched} 項` : ''}
                            onInput=${(e) => set({ statText: e.currentTarget.value })}
                        />
                        ${
                            /* 原程式這一列就是一條到底的凹槽，沒有旁註：認齊了不出聲，
                              認不齊才擠一句出來（欄位同時會轉紅框）。 */ ''
                        }
                        ${
                            s.statText &&
                            !parsed.ok &&
                            html`<span class="parse-note bad">只認出 ${parsed.matched} 項</span>`
                        }
                    <//>

                    <${Row} label="運算模式">
                        <${BtnRow}
                            options=${constants.calc_modes.map((m) => ({
                                key: m.key,
                                label: m.label,
                                title: MODE_HINT[m.key],
                            }))}
                            value=${s.calcMode}
                            onChange=${(calcMode) => set({ calcMode })}
                        />
                    <//>

                    <footer class="status">${status}</footer>
                <//>
            </div>

            ${tab === 'expand' && html`<${StatStrip} row=${preview} constants=${constants} />`}
            <${GuessResults} resp=${guessResp} constants=${constants} />
            <${SeriesTable} rows=${seriesRows} constants=${constants} />

            ${
                showCatalog &&
                html`
                <${CatalogWindow}
                    catalog=${catalog}
                    ${/* 種族名稱與碼從這裡來 —— 前端不留第二份 */ ''}
                    constants=${constants}
                    customPath=${boot.custom_path}
                    ${/* 能力搜尋的等級預設值 —— 帶主視窗現在那級，省一次輸入 */ ''}
                    lvl=${s.lvl}
                    random=${s.random}
                    onCatalog=${setCatalog}
                    onPick=${(p) => {
                        pickPet(p);
                        setShowCatalog(false);
                    }}
                    onClose=${() => setShowCatalog(false)}
                    onStatus=${setStatus}
                />
            `
            }

            ${
                /* 出處。原程式與圖鑑都是別人的心血，掛在畫面上而不是只寫在 README 裡。
                  ⚠️ htm 會把換行處的空白吃掉，所以每一段連同前後的空格一起寫成
                  字串常數，不要為了排版把 <a> 拆到下一行。 */ ''
            }
            <footer class="credit">
                ${'重製自「噬生・魔物觀測者」v3.12 ・ 圖鑑資料來自 '}
                <a href="https://github.com/cg-originmood-dc/cg-originmood-dc.github.io" rel="noreferrer">cg-originmood-dc</a>
                ${' ・ 計算對照 '}
                <a href="https://github.com/tony1223/cg-pet-calc" rel="noreferrer">cg-pet-calc</a>
            </footer>
        </div>
    `;
}

const MODE_HINT = {
    smart: '智：家寵，等級點自由分配',
    wild: '野：野寵，等級點由系統隨機分配',
    none: '無：完全未加點',
    hp: '體：每升一級都加體力',
    atk: '力：每升一級都加力量',
    def: '防：每升一級都加強度',
    agi: '敏：每升一級都加速度',
    mp: '魔：每升一級都加魔法',
};

const HELP = '填檔次與當前能力後按「計算」推算掉檔／加點／隨機檔；左邊分頁可展開進階條件。';

// ── 請求組裝 ────────────────────────────────────────────────────────────────

const toGrowDto = (grow, bprate100) => ({
    hp: grow[0],
    atk: grow[1],
    def: grow[2],
    agi: grow[3],
    mp: grow[4],
    bprate: bprate100 / 100,
});

/**
 * 「入手等級 N」＝ 1→N 的點是系統配的，不算玩家加點。
 *
 * 入手等級不可能高過當前等級，但兩個欄位是分開改的，所以在這裡夾一次 ——
 * 靠每個 setter 各自維護不變條件，遲早會漏掉一個。
 */
const notOrderPoint = (s) => Math.max(0, Math.min(s.catchLvl, s.lvl) - 1);

/**
 * 「入手能力」轉成後端要的觀測值。
 *
 * 勾了「使用入手能力」而且那串真的認得出五項時才成立，否則回 `null`
 * ＝ 不加這個條件。入手等級跟當前等級是分開改的，所以這裡跟 `notOrderPoint`
 * 用同一個夾法 —— 後端會擋掉超界的入手等級，前端先夾好就撞不到那個錯誤。
 */
function buildCatchTarget(s, parsedCatch) {
    if (!s.useCatchStat || !parsedCatch.ok) return null;
    const v = parsedCatch.values;
    return {
        lvl: Math.max(1, Math.min(s.catchLvl, s.lvl)),
        hp: v.hp,
        mp: v.mp,
        atk: v.atk,
        def: v.def,
        agi: v.agi,
    };
}

function forwardBase(s, grow) {
    return {
        grow,
        lvl: s.lvl,
        random: s.random,
        mode: s.growMode,
        manual: null,
        not_order_point: notOrderPoint(s),
        // 「下次」＝ 不換軸，整段都照加點走（後端把 null 當成這個意思）
        burst: s.burst === 'next' ? null : s.burst,
    };
}

function buildGuessReq(s, grow, parsed, planEditable, catchTarget) {
    const v = parsed.values;
    return {
        grow,
        target: { lvl: s.lvl, hp: v.hp, mp: v.mp, atk: v.atk, def: v.def, agi: v.agi },
        not_order_point: notOrderPoint(s),
        calc_mode: s.calcMode,
        manual: planEditable ? manualPlan(s) : null,
        catch_stat: catchTarget,
        mode: s.matchMode,
        // 野寵的隨機檔完全未知，代數估計的 ±1 鄰域接不住，一律窮舉 1001 組。
        exhaustive: s.calcMode === 'wild',
        tolerance: null,
        target_grow: null,
        limit: 0,
        ranges: s.ranges,
    };
}

function manualPlan(s) {
    if (s.plan === 'fixed') return { kind: 'fixed', points: s.fixedPoints };
    if (s.plan === 'mixed') {
        // 沒勾的軸上界壓成 0，勾了的軸用使用者給的範圍
        const bounds = s.mixedAxes.map((on, i) =>
            on ? [Math.max(0, s.mixedLo[i]), Math.max(s.mixedLo[i], s.mixedHi[i])] : [0, 0],
        );
        return { kind: 'range', bounds };
    }
    return { kind: 'free' };
}

/** 放寬過才附註 —— `exact` 是常態，講了只是雜訊。 */
const MATCH_NOTE = {
    observer: '（魔觀規則：修過不規則檔次的取整邊界）',
    tolerant: '（寬鬆比對：比原程式鬆，僅供參考）',
};

function describeGuess(resp, usedCatch) {
    if (resp.total === 0) {
        // 帶了入手能力還無解時，最可疑的就是那串 —— 它比當前能力更難填對
        // （沒有加點可以吸收誤差），先指向它比叫使用者亂猜有用。
        return usedCatch
            ? '推不出解 —— 入手能力也得對得上，先關掉「使用入手能力」試試'
            : '推不出解 —— 檢查能力值、等級與入手等級';
    }
    const top = resp.candidates[0];
    const head = `${resp.total} 組解，最可能：檔次 ${top.grow.join(' ')}（${top.percent.toFixed(2)}%）`;
    const tail = resp.distribution?.fully_determined ? `${head}　掉檔已唯一` : head;
    // 精確模式不用講；有放寬就講，使用者才知道這批解是「修過取整邊界」來的
    return MATCH_NOTE[resp.mode] ? `${tail}　${MATCH_NOTE[resp.mode]}` : tail;
}

async function save(s, preview, setStatus) {
    const text = preview ? formatStats(preview.stat) : s.statText;
    if (!text) return setStatus('沒有可以保存的能力值');
    try {
        await navigator.clipboard.writeText(text);
        setStatus(`已複製：${text}`);
    } catch {
        setStatus(`複製失敗，內容是：${text}`);
    }
}

// ── 正算預覽 ────────────────────────────────────────────────────────────────

/**
 * 「擴展」頁的即時能力預覽。
 *
 * 每次改欄位都往後端跑一趟；120ms 的抖動抑制夠讓連打數字時只送最後一次。
 * 回應可能亂序（後端跑在別的執行緒），所以用序號擋掉過期的結果。
 */
function useLivePreview({ s, growDto, active }) {
    const [row, setRow] = useState(null);
    const seq = useRef(0);

    const key = JSON.stringify([growDto, s.lvl, s.random, s.growMode, s.burst, notOrderPoint(s)]);

    useEffect(() => {
        if (!active) return undefined;
        const mine = ++seq.current;
        const t = setTimeout(async () => {
            try {
                const r = await api.forward(forwardBase(s, growDto));
                if (mine === seq.current) setRow(r);
            } catch {
                if (mine === seq.current) setRow(null);
            }
        }, 120);
        return () => clearTimeout(t);
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [key, active]);

    return active ? row : null;
}
