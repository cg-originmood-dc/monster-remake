// 「觀測者・寵物資料」視窗。
//
// 三個畫面，跟原程式一樣互相取代（原程式是清單面板與編輯面板兩塊交替顯隱）：
//
// * **清單** —— 種族分頁 ＋ 清單 ＋ 搜尋，選一隻回填主視窗
// * **編輯** —— 魔物名字／五項檔次／能力倍率／地水火風／種族／技能
// * **能力搜尋** —— 原程式主視窗的寵物搜尋面板，「搜索不低於輸入能力的寵物」
//
// ## 能力搜尋為什麼放在這個視窗
//
// 原程式的輸入欄位掛在主視窗，但**結果是列在寵物資料視窗的清單裡**的。
// 移植版把輸入也搬過來：輸入與結果隔著兩個視窗的話，改一個數字就要來回看兩邊。
// 十個欄位在這裡放得下，主視窗放不下。
//
// 分頁順序照原程式的排法（`petdata::RACE_TAB_ORDER`），由後端隨圖鑑一起送過來，
// 前端不重排。**種族名字以線上圖鑑那份為準**，所以連名字也不在這裡寫死。
//
// ## 能改的只有自製寵物
//
// 原程式也是這樣分的：`[自製寵物]` 段的記錄可以改可以刪，內建表是
// 把整份資料當字串常數寫死在 exe 裡的，沒有路徑能動它。
//
// 移植版把這條線畫得寬鬆一點：**改上游圖鑑裡的寵物會變成一筆同名的自製記錄蓋上去**，
// 把那筆刪掉，上游的就露回來。線上那份圖鑑是每次開啟時抓的，**唯讀**。
// 自製寵物存在 UTF-8 的 `custom_pets.json` 裡（網頁版是 localStorage），
// 路徑由 bootstrap 帶過來顯示在視窗上。

import { useEffect, useMemo, useRef, useState } from 'preact/hooks';
import * as api from './api.js';
import { html, Btn, NumField, SpinField, TextField } from './ui.js';

const AXIS_NAMES = ['體力', '力量', '強度', '速度', '魔法'];
const ELEMENT_NAMES = ['地', '水', '火', '風'];

/** 能力搜尋的七個欄位。順序就是 `petcalc::STATS` 的順序，前端不重排。 */
const STAT_NAMES = ['生命', '魔力', '攻擊', '防禦', '敏捷', '精神', '回復'];
const STAT_KEYS = ['hp', 'mp', 'atk', 'def', 'agi', 'wis', 'res'];

export function CatalogWindow({
    catalog,
    constants,
    customPath,
    lvl = 1,
    random,
    onCatalog,
    onPick,
    onClose,
    onStatus,
}) {
    const [race, setRace] = useState(catalog.races[0]?.code ?? 0);
    const [keyword, setKeyword] = useState('');
    const [found, setFound] = useState(null);
    const [sel, setSel] = useState(null);
    /** 編輯中的草稿；`null` ＝ 不在編輯畫面。 */
    const [draft, setDraft] = useState(null);
    /** 能力搜尋的條件；`null` ＝ 不在搜尋畫面。 */
    const [finder, setFinder] = useState(null);
    const [error, setError] = useState(null);
    const [busy, setBusy] = useState(false);
    const boxRef = useRef(null);

    useEffect(() => {
        // 編輯／搜尋到一半按 Esc 只退回清單，不關視窗 —— 不然剛打的東西整批不見
        const onKey = (e) => {
            if (e.key !== 'Escape') return;
            if (draft) return setDraft(null);
            if (finder) return setFinder(null);
            onClose();
        };
        document.addEventListener('keydown', onKey);
        return () => document.removeEventListener('keydown', onKey);
    }, [onClose, draft, finder]);

    const list = useMemo(
        () => found ?? catalog.pets.filter((p) => p.race === race),
        [found, catalog.pets, race],
    );

    /** 編輯畫面的種族選項。名字跟著圖鑑走，所以從後端的常數取。 */
    const races = useMemo(() => raceOptions(constants), [constants]);

    // 換分頁／換搜尋結果時，捲軸與選取都要歸零，不然會停在上一份清單的位置
    useEffect(() => {
        setSel(null);
        if (boxRef.current) boxRef.current.scrollTop = 0;
    }, [race, found]);

    const doSearch = async () => {
        const k = keyword.trim();
        if (!k) return setFound(null);
        try {
            setFound(await api.searchPets(k));
        } catch (e) {
            onStatus(String(e));
        }
    };

    /**
     * 重抓線上圖鑑。開啟時已經自動抓過一次了，這顆是**抓失敗時的退路** ——
     * 網路斷一下就得整頁重載未免太兇。
     */
    const reload = async () => {
        setBusy(true);
        try {
            const c = await api.loadCatalog();
            onCatalog(c);
            setFound(null);
            setRace(c.races[0]?.code ?? 0);
            onStatus(`已載入 ${c.count} 隻：${c.source}`);
        } catch (e) {
            onStatus(String(e));
        } finally {
            setBusy(false);
        }
    };

    /** 存檔後圖鑑會整份換掉，選取要用名字重新對回去，不能留著舊物件。 */
    const adopt = (c, name) => {
        onCatalog(c);
        setFound(null);
        setSel(c.pets.find((p) => p.name === name) ?? null);
    };

    const save = async () => {
        try {
            const c = await api.savePet(toPet(draft), draft.renameFrom);
            adopt(c, draft.name.trim());
            setDraft(null);
            setError(null);
            onStatus(`已保存「${draft.name.trim()}」`);
        } catch (e) {
            // 後端才是權威（範圍、地水火風總和都在那邊驗），錯誤原樣顯示在表單上
            setError(String(e));
        }
    };

    const remove = async () => {
        try {
            const c = await api.deletePet(sel.name);
            const back = c.pets.find((p) => p.name === sel.name);
            adopt(c, sel.name);
            onStatus(back ? `已還原上游的「${sel.name}」` : `已刪除「${sel.name}」`);
        } catch (e) {
            onStatus(String(e));
        }
    };

    return html`
        <div class="backdrop" onClick=${(e) => e.target === e.currentTarget && onClose()}>
            <section class="panel window">
                <header class="panel-title">
                    <span>
                        ${draft ? '寵物檔案' : finder ? '觀測者・寵物搜尋' : '觀測者・寵物資料'}
                    </span>
                    <span class="panel-icons">
                        <button type="button" class="tiny-icon" onClick=${onClose}>×</button>
                    </span>
                </header>
                <div class="panel-body">
                    ${
                        finder
                            ? html`<${StatFinder}
                              init=${finder}
                              onPick=${onPick}
                              onStatus=${onStatus}
                              onClose=${() => setFinder(null)}
                          />`
                            : draft
                              ? html`<${PetForm}
                              draft=${draft}
                              set=${(patch) => setDraft({ ...draft, ...patch })}
                              error=${error}
                              races=${races}
                              customPath=${customPath}
                              onSave=${save}
                              onCancel=${() => {
                                  setDraft(null);
                                  setError(null);
                              }}
                          />`
                              : html`
                              <div class="cat-source">
                                  ${/* 疊完的來源字串由後端組（內建 ＋ 線上），前端不自己拼 */ ''}
                                  <span class="cat-src-label" title=${catalog.source}>
                                      ${catalog.source}
                                      <b>${catalog.count} 隻</b>
                                  </span>
                                  <${Btn}
                                      disabled=${busy}
                                      title="線上圖鑑是每次開啟時抓的；沒抓到的話按這裡再試一次"
                                      onClick=${reload}
                                  >
                                      ${busy ? '載入中' : '重載圖鑑'}
                                  <//>
                                  <${Btn}
                                      title="加一隻圖鑑裡沒有的寵物"
                                      onClick=${() => {
                                          setError(null);
                                          setDraft(blankDraft(race, races));
                                      }}
                                  >
                                      添加寵物
                                  <//>
                                  <${Btn}
                                      title="搜索不低於輸入能力的寵物"
                                      onClick=${() => setFinder({ lvl, random })}
                                  >
                                      能力搜尋
                                  <//>
                              </div>

                              <div class="cat-search">
                                  <input
                                      type="text"
                                      class="field grow"
                                      value=${keyword}
                                      placeholder="搜尋寵物名稱"
                                      onInput=${(e) => setKeyword(e.currentTarget.value)}
                                      onKeyDown=${(e) => e.key === 'Enter' && doSearch()}
                                  />
                                  <${Btn} onClick=${doSearch}>搜尋<//>
                                  ${
                                      found &&
                                      html`<${Btn}
                                      onClick=${() => {
                                          setKeyword('');
                                          setFound(null);
                                      }}
                                  >
                                      清除
                                  <//>`
                                  }
                              </div>

                              <div class="cat-grid">
                                  <nav class="cat-races">
                                      ${catalog.races.map(
                                          (r) => html`
                                              <button
                                                  key=${r.code}
                                                  type="button"
                                                  class="race ${
                                                      !found && r.code === race ? 'on' : ''
                                                  }"
                                                  disabled=${r.count === 0}
                                                  onClick=${() => {
                                                      setFound(null);
                                                      setRace(r.code);
                                                  }}
                                              >
                                                  ${r.name}<b>${r.count}</b>
                                              </button>
                                          `,
                                      )}
                                  </nav>

                                  <div class="cat-list" ref=${boxRef}>
                                      ${
                                          list.length === 0
                                              ? html`<p class="empty">這個分類沒有寵物</p>`
                                              : list.map(
                                                    (p, i) => html`
                                                    <div
                                                        key=${p.name + i}
                                                        class="cat-item ${
                                                            sel?.name === p.name ? 'on' : ''
                                                        }"
                                                        onClick=${() => setSel(p)}
                                                        onDblClick=${() => onPick(p)}
                                                    >
                                                        <span class="c-name">
                                                            ${p.name}
                                                            ${
                                                                p.custom &&
                                                                html`<b class="mine" title="自製寵物"
                                                                >★</b
                                                            >`
                                                            }
                                                        </span>
                                                        <span class="c-grow">
                                                            ${p.grow.join(' ')}
                                                        </span>
                                                        <span class="c-sum">${p.grow_sum}</span>
                                                    </div>
                                                `,
                                                )
                                      }
                                  </div>

                                  <aside class="cat-detail">
                                      ${
                                          sel
                                              ? html`<${PetDetail}
                                                pet=${sel}
                                                onPick=${() => onPick(sel)}
                                                onEdit=${() => {
                                                    setError(null);
                                                    setDraft(draftFrom(sel));
                                                }}
                                                onDelete=${remove}
                                            />`
                                              : html`<p class="empty">從清單選一隻</p>`
                                      }
                                  </aside>
                              </div>
                          `
                    }
                </div>
            </section>
        </div>
    `;
}

// ── 明細 ────────────────────────────────────────────────────────────────────

const PetDetail = ({ pet, onPick, onEdit, onDelete }) => html`
    <h4>${pet.name}${pet.custom && html`<b class="mine">★</b>`}</h4>
    <dl>
        <dt>編號</dt>
        <dd>${pet.id ?? '—'}</dd>
        <dt>種族</dt>
        <dd>${pet.race_name}</dd>
        <dt>能力倍率</dt>
        <dd>${Math.round(pet.bprate * 100)}</dd>
        <dt>技能格</dt>
        <dd>${pet.skills ?? '—'}</dd>
        <dt>檔次和</dt>
        <dd>${pet.grow_sum}</dd>
    </dl>
    <div class="grow-list">
        ${AXIS_NAMES.map((n, i) => html`<span key=${n}><b>${n}</b>${pet.grow[i]}</span>`)}
    </div>
    ${
        pet.element &&
        html`<div class="grow-list elem">
        ${ELEMENT_NAMES.map((n, i) => html`<span key=${n}><b>${n}</b>${pet.element[i]}</span>`)}
    </div>`
    }
    <${Btn} wide onClick=${onPick}>選這隻<//>
    <div class="cat-actions">
        <${Btn} onClick=${onEdit}>修改<//>
        <${Btn}
            tone="danger"
            disabled=${!pet.custom}
            title=${
                pet.custom
                    ? '刪掉這筆自製記錄'
                    : '圖鑑本身刪不了 —— 按「修改」會蓋一筆自製的上去，那筆才能刪'
            }
            onClick=${onDelete}
        >
            刪除
        <//>
    </div>
    ${
        !pet.custom &&
        html`<p class="hint">改這隻會存成一筆自製記錄蓋在上面，圖鑑檔本身不會被寫到。</p>`
    }
`;

// ── 能力搜尋 ────────────────────────────────────────────────────────────────

/**
 * 「搜索不低於輸入能力的寵物」（原程式主視窗的寵物搜尋面板）。
 *
 * 判定的是**補得到**而不是**天生就有**：一隻寵在該等級不加點的能力算出來後，
 * 每項差多少就換算成加點數（走該項最有效率的那一軸），合計 ≤ `等級-1` 就算命中。
 * 這條規則與係數對照見 `crates/petcalc/src/search.rs`，那裡有跟 binary 的逐項對照。
 *
 * 隨機檔用主視窗當下那組 —— 原程式用哪組沒查出來，但至少跟畫面上其他地方一致。
 */
function StatFinder({ init, onPick, onStatus, onClose }) {
    // 原程式要求等級 > 1 —— 1 級沒有加點可用，整個搜尋沒有意義。
    // 主視窗剛開起來就是 1 級，所以帶過來的值要先抬到下限。
    const [lvl, setLvl] = useState(Math.max(2, init.lvl));
    const [floor, setFloor] = useState([0, 0, 0, 0, 0, 0, 0]);
    const [skills, setSkills] = useState(0);
    const [element, setElement] = useState(0);
    const [resp, setResp] = useState(null);
    const [sel, setSel] = useState(null);
    const [busy, setBusy] = useState(false);

    const run = async () => {
        setBusy(true);
        try {
            const r = await api.searchByStats({ lvl, floor, skills, element, random: init.random });
            setResp(r);
            setSel(null);
            onStatus(
                r.total === 0
                    ? `${lvl} 級沒有寵物補得到這組能力`
                    : `${lvl} 級有 ${r.total} 隻補得到（可用 ${r.available} 點）`,
            );
        } catch (e) {
            onStatus(String(e));
        } finally {
            setBusy(false);
        }
    };

    // 用函式版更新：七個欄位擠在一排，連續改兩格（貼上、Tab 快速跳）時
    // 兩次 setState 會讀到同一份舊 floor，後面那次把前面那次蓋掉。
    const setStat = (i, v) => setFloor((prev) => prev.map((o, j) => (j === i ? v : o)));

    return html`
        <div class="finder">
            <p class="hint inline">搜索不低於輸入能力的寵物 —— 空著（0）的項目不限。</p>

            <div class="pf-row">
                <span>目標等級</span>
                <${SpinField} value=${lvl} onChange=${setLvl} min=${2} max=${255} />
                <span>技能</span>
                <${NumField} value=${skills} min=${0} max=${20} onChange=${setSkills} />
                <span>屬性</span>
                <select
                    class="field pick"
                    value=${element}
                    onChange=${(e) => setElement(Number(e.currentTarget.value))}
                >
                    <option value=${0}>不限</option>
                    ${ELEMENT_NAMES.map(
                        (n, i) => html`<option key=${n} value=${i + 1}>${n}</option>`,
                    )}
                </select>
            </div>

            <div class="pf-row">
                <span>能力下限</span>
                <div class="pf-axes">
                    ${STAT_NAMES.map(
                        (n, i) => html`
                            <label key=${n}>
                                <${NumField}
                                    value=${floor[i]}
                                    min=${0}
                                    max=${9999}
                                    onChange=${(v) => setStat(i, v)}
                                />
                                <span>${n}</span>
                            </label>
                        `,
                    )}
                </div>
                <${Btn} tone="go" disabled=${busy} onClick=${run}>${busy ? '搜尋中' : '搜尋'}<//>
                <${Btn} onClick=${onClose}>返回<//>
            </div>

            <div class="cat-grid narrow">
                <div class="cat-list">
                    ${
                        !resp
                            ? html`<p class="empty">填好能力下限後按「搜尋」</p>`
                            : resp.hits.length === 0
                              ? html`<p class="empty">沒有寵物補得到這組能力</p>`
                              : resp.hits.map(
                                    (h, i) => html`
                                  <div
                                      key=${h.pet.name + i}
                                      class="cat-item ${sel?.pet.name === h.pet.name ? 'on' : ''}"
                                      onClick=${() => setSel(h)}
                                      onDblClick=${() => onPick(h.pet)}
                                  >
                                      <span class="c-name">
                                          ${h.pet.name}
                                          ${
                                              h.pet.custom &&
                                              html`<b class="mine" title="自製寵物">★</b>`
                                          }
                                      </span>
                                      ${/* 剩餘點數就是排序鍵，越多越有餘裕 */ ''}
                                      <span class="c-grow">需 ${h.needed}</span>
                                      <span class="c-sum">餘 ${h.spare}</span>
                                  </div>
                              `,
                                )
                    }
                </div>

                <aside class="cat-detail">
                    ${
                        sel
                            ? html`<${HitDetail} hit=${sel} lvl=${lvl} onPick=${() => onPick(sel.pet)} />`
                            : html`<p class="empty">
                              ${
                                  resp
                                      ? `共 ${resp.total} 隻${resp.truncated ? '（已截斷）' : ''}`
                                      : '尚未搜尋'
                              }
                          </p>`
                    }
                </aside>
            </div>
        </div>
    `;
}

/** 一筆命中的明細：該等級不加點的能力，以及還差多少／還剩多少點。 */
const HitDetail = ({ hit, lvl, onPick }) => html`
    <h4>${hit.pet.name}${hit.pet.custom && html`<b class="mine">★</b>`}</h4>
    <dl>
        <dt>種族</dt>
        <dd>${hit.pet.race_name}</dd>
        <dt>檔次和</dt>
        <dd>${hit.pet.grow_sum}</dd>
        <dt>需要加點</dt>
        <dd>${hit.needed}</dd>
        <dt>剩餘點數</dt>
        <dd>${hit.spare}</dd>
    </dl>
    ${/* 這排是「完全不加點」的能力，加點怎麼配由使用者自己決定 */ ''}
    <p class="hint inline">${lvl} 級未加點</p>
    <div class="grow-list">
        ${STAT_NAMES.map(
            (n, i) => html`<span key=${n}><b>${n}</b>${hit.stat[STAT_KEYS[i]]}</span>`,
        )}
    </div>
    <p class="hint inline">檔次</p>
    <div class="grow-list">
        ${AXIS_NAMES.map((n, i) => html`<span key=${n}><b>${n}</b>${hit.pet.grow[i]}</span>`)}
    </div>
    <${Btn} wide onClick=${onPick}>選這隻<//>
`;

// ── 編輯 ────────────────────────────────────────────────────────────────────

/**
 * 欄位順序照原程式的編輯畫面：
 * 魔物名字 → 五項檔次 → 能力倍率 → 地水火風 → 種族／技能。
 *
 * 原程式那裡**沒有編號欄位** —— 編號是存檔時自己找一個沒用過的號碼配上去的。
 * 移植版連配都不配：編號是動畫資源的鍵，自己加的寵物本來就沒有對應的動畫，
 * 編一個假的只會誤導。
 */
const PetForm = ({ draft, set, error, races, customPath, onSave, onCancel }) => {
    const sum = draft.element.reduce((a, b) => a + b, 0);
    // 全 0 ＝ 沒填，是允許的；填了就得剛好 100
    const elementBad = sum !== 0 && sum !== 100;

    return html`
        <div class="pet-form">
            <label class="pf-row">
                <span>魔物名字</span>
                <${TextField}
                    className="grow"
                    value=${draft.name}
                    placeholder="未命名"
                    onInput=${(name) => set({ name })}
                />
            </label>

            <div class="pf-row">
                <span>檔次</span>
                <div class="pf-axes">
                    ${AXIS_NAMES.map(
                        (n, i) => html`
                            <label key=${n}>
                                <${NumField}
                                    value=${draft.grow[i]}
                                    min=${0}
                                    max=${110}
                                    onChange=${(v) =>
                                        set({ grow: draft.grow.map((g, j) => (j === i ? v : g)) })}
                                />
                                <span>${n}</span>
                            </label>
                        `,
                    )}
                    <span class="rest">和 ${draft.grow.reduce((a, b) => a + b, 0)}</span>
                </div>
            </div>

            <div class="pf-row">
                <span>能力倍率</span>
                <${NumField}
                    value=${draft.bprate}
                    min=${1}
                    max=${100}
                    size="md"
                    onChange=${(bprate) => set({ bprate })}
                />
                ${/* 原程式這一格旁邊就印著「※不須改動」，全圖鑑 1200 多隻恆為 20 */ ''}
                <span class="hint inline">※不須改動，恆為 20</span>
            </div>

            <div class="pf-row">
                <span>屬性</span>
                <div class="pf-axes">
                    ${ELEMENT_NAMES.map(
                        (n, i) => html`
                            <label key=${n}>
                                <${NumField}
                                    value=${draft.element[i]}
                                    min=${0}
                                    max=${100}
                                    onChange=${(v) =>
                                        set({
                                            element: draft.element.map((e, j) => (j === i ? v : e)),
                                        })}
                                />
                                <span>${n}</span>
                            </label>
                        `,
                    )}
                    <span class="rest ${elementBad ? 'bad' : ''}">
                        ${sum === 0 ? '未填' : `合計 ${sum} / 100`}
                    </span>
                </div>
            </div>

            <div class="pf-row">
                <span>種族</span>
                <select
                    class="field pick"
                    value=${draft.race}
                    onChange=${(e) => set({ race: Number(e.currentTarget.value) })}
                >
                    ${races.map(
                        (r) => html`<option key=${r.code} value=${r.code}>${r.name}</option>`,
                    )}
                </select>
                <span>技能</span>
                <${NumField}
                    value=${draft.skills}
                    min=${0}
                    max=${20}
                    onChange=${(skills) => set({ skills })}
                />
            </div>

            ${error && html`<p class="pf-error">${error}</p>`}

            <div class="pf-actions">
                <${Btn} tone="go" disabled=${elementBad} onClick=${onSave}>保存<//>
                <${Btn} onClick=${onCancel}>放棄<//>
            </div>

            <p class="hint">
                存進 <code>${customPath ?? 'custom_pets.json'}</code>（UTF-8）——
                上游的 <code>spetdata.ini</code> 不會被動到。
            </p>
        </div>
    `;
};

/**
 * 種族下拉的選項：`constants.race_names` 切掉最後一筆。
 *
 * 不從 `catalog.races` 取，是因為那份是**分頁**清單 —— 帶著每個分頁的筆數、
 * 順序照原程式排、而且含「其他」。「其他」是給沒有種族資訊的舊資料落腳用的容器，
 * 不該讓使用者主動挑，所以這裡切掉。索引即種族碼（`unknown_race` 一定在最後一筆，
 * 由 `dto.rs::the_race_list_is_indexed_by_code_with_unknown_last` 釘住）。
 */
const raceOptions = (constants) =>
    constants.race_names.slice(0, constants.unknown_race).map((name, code) => ({ code, name }));

/** 「添加寵物」的空白表單。種族先帶目前分頁，省一次操作。 */
function blankDraft(race, races) {
    return {
        renameFrom: null,
        name: '',
        race: races.some((r) => r.code === race) ? race : 0,
        grow: [0, 0, 0, 0, 0],
        bprate: 20,
        element: [0, 0, 0, 0],
        skills: 0,
        id: null,
    };
}

/** 「修改」：把現有的一隻攤成草稿。`renameFrom` 記著舊名字，因為名字就是鍵。 */
function draftFrom(pet) {
    return {
        renameFrom: pet.name,
        name: pet.name,
        race: pet.race,
        grow: [...pet.grow],
        bprate: Math.round(pet.bprate * 100),
        element: pet.element ? [...pet.element] : [0, 0, 0, 0],
        skills: pet.skills ?? 0,
        id: pet.id,
    };
}

/** 草稿 → 後端的 PetDto。倍率在畫面上是 ×100 的整數（跟 INI 一樣）。 */
function toPet(draft) {
    const sum = draft.element.reduce((a, b) => a + b, 0);
    return {
        id: draft.id,
        name: draft.name.trim(),
        race: draft.race,
        grow: draft.grow,
        bprate: draft.bprate / 100,
        skills: draft.skills || null,
        // 全 0 ＝ 使用者沒填，送 null 而不是四個 0 —— 後端會擋「加起來不是 100」
        element: sum === 0 ? null : draft.element,
    };
}
