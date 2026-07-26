// 「寵物名字」欄位 —— 邊打邊出候選清單。
//
// 篩選是在前端做的：整份圖鑑（755 隻）啟動時就隨 bootstrap 一起送過來了，
// 每敲一個鍵就往後端跑一趟只會讓下拉選單卡住，還得處理回應亂序的問題。
// 後端的 search_pets 留給「寵物檔案」視窗的明確搜尋用。

import { useEffect, useMemo, useRef, useState } from 'preact/hooks';
import { html } from './ui.js';

const MAX_SUGGESTIONS = 40;

export function PetPicker({ pets, value, onPick, placeholder = '寵物名字', className = '' }) {
    const [text, setText] = useState(value ?? '');
    const [open, setOpen] = useState(false);
    const [cursor, setCursor] = useState(0);
    const boxRef = useRef(null);

    // 外部換寵物（例如從寵物檔案視窗選）時要跟著更新
    useEffect(() => setText(value ?? ''), [value]);

    // 點到別的地方就收起來
    useEffect(() => {
        const onDown = (e) => {
            if (boxRef.current && !boxRef.current.contains(e.target)) setOpen(false);
        };
        document.addEventListener('pointerdown', onDown);
        return () => document.removeEventListener('pointerdown', onDown);
    }, []);

    const hits = useMemo(() => {
        const k = text.trim();
        if (!k) return [];
        // 前綴命中排在包含命中前面 —— 打「火」時「火焰牛鬼領主」該比「烈火牛」先出現
        const exact = [];
        const inner = [];
        for (const p of pets) {
            const i = p.name.indexOf(k);
            if (i === 0) exact.push(p);
            else if (i > 0) inner.push(p);
            if (exact.length >= MAX_SUGGESTIONS) break;
        }
        return [...exact, ...inner].slice(0, MAX_SUGGESTIONS);
    }, [pets, text]);

    const choose = (p) => {
        setText(p.name);
        setOpen(false);
        onPick(p);
    };

    const onKeyDown = (e) => {
        if (!open || hits.length === 0) return;
        if (e.key === 'ArrowDown') {
            e.preventDefault();
            setCursor((c) => (c + 1) % hits.length);
        } else if (e.key === 'ArrowUp') {
            e.preventDefault();
            setCursor((c) => (c - 1 + hits.length) % hits.length);
        } else if (e.key === 'Enter') {
            e.preventDefault();
            choose(hits[Math.min(cursor, hits.length - 1)]);
        } else if (e.key === 'Escape') {
            setOpen(false);
        }
    };

    return html`
        <div class="petpicker" ref=${boxRef}>
            <input
                type="text"
                class="field wide ${className}"
                value=${text}
                placeholder=${placeholder}
                onInput=${(e) => {
                    setText(e.currentTarget.value);
                    setCursor(0);
                    setOpen(true);
                }}
                onFocus=${() => setOpen(true)}
                onKeyDown=${onKeyDown}
            />
            ${
                open &&
                hits.length > 0 &&
                html`
                <ul class="suggest">
                    ${hits.map(
                        (p, i) => html`
                            <li
                                key=${p.name + i}
                                class="suggest-item ${i === cursor ? 'on' : ''}"
                                onPointerEnter=${() => setCursor(i)}
                                onClick=${() => choose(p)}
                            >
                                <span class="s-name">${p.name}</span>
                                <span class="s-grow">${p.grow.join(' ')}</span>
                                <span class="s-race">${p.race_name}</span>
                            </li>
                        `,
                    )}
                </ul>
            `
            }
        </div>
    `;
}
