// 進入點：先跟 Rust 端要一次 bootstrap（圖鑑＋常數），拿到才掛主視窗。
//
// 常數（成長係數表、容差上下界、軸名、運算模式清單）刻意不在前端硬編 ——
// 前端多留一份就多一次跟 Rust 走鐘的機會，那正是 cg-pet-calc 那張殘表的下場。

import { render } from 'preact';
import * as api from './api.js';
import { html } from './ui.js';
import { App } from './app.js';

const root = document.getElementById('app');

render(html`<${Splash} />`, root);

api.bootstrap().then(
    (boot) => render(html`<${App} boot=${boot} />`, root),
    (err) => render(html`<${Failed} err=${err} />`, root),
);

function Splash() {
    return html`<div class="splash">觀測中…</div>`;
}

function Failed({ err }) {
    return html`
        <div class="splash failed">
            <h2>起不來</h2>
            <pre>${String(err?.message ?? err)}</pre>
            <p>
                計算核心是編成 WebAssembly 的 Rust，跑在 Web Worker 裡。
                起不來通常是這兩件事之一：瀏覽器太舊（需要 module worker），
                或是 <code>wasm-pkg/</code> 還沒建 —— 那就跑一次
                <code>npm run build:wasm</code>。
            </p>
        </div>
    `;
}
