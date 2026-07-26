import { defineConfig } from 'vite';

export default defineConfig({
    // 相對路徑 —— GitHub Pages 的專案頁在 /<repo>/ 底下，寫死 '/' 會全部 404。
    // 相對的話 user page（根目錄）跟 project page（子目錄）都對。
    base: './',
    server: {
        port: 5173,
    },
    worker: {
        // worker.js 用 import 拉 wasm 膠水碼，classic worker 讀不了 import
        format: 'es',
    },
    build: {
        // wasm-bindgen 的膠水碼有 top-level await，需要比 Vite 預設更新的目標
        target: 'es2022',
    },
});
