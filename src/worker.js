// wasm 引擎的宿主。
//
// 跑在 Web Worker 裡是刻意的：推算最壞情況要列舉 3125 組檔次 × 加點 × 1001 組
// 隨機檔，會跑好幾秒。擺在主執行緒的話畫面會整個凍住，連「計算中」都畫不出來。
//
// 這裡不做任何判斷 —— 指令名與參數原封不動丟給 wasm，回傳原封不動送回去。
// 唯一多做的是把「自製寵物有沒有變動」一起帶回主執行緒：
// Worker 拿不到 localStorage（規格如此，不是 bug），所以寫檔那一步得由外面做。

import init, { Observer } from '../wasm-pkg/petweb.js';

let observer = null;

self.onmessage = async ({ data: { id, cmd, args, boot } }) => {
    try {
        if (cmd === '__init') {
            await init();
            observer = new Observer(boot.custom ?? null);
            return reply({ id, ok: true, value: null });
        }
        if (!observer) throw new Error('引擎還沒起來');

        const value = observer.invoke(cmd, args ?? {});
        // takeDirty 會把旗標清掉，所以每次呼叫完都要問 —— 漏問一次就漏存一次
        reply({ id, ok: true, value, persist: observer.takeDirty() ?? null });
    } catch (e) {
        reply({ id, ok: false, error: String(e?.message ?? e) });
    }
};

function reply(msg) {
    self.postMessage(msg);
}
