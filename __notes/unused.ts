/**
 * Linuxでは細長表示になってしまったため、使用を中止
 */

const { getCurrentWindow, LogicalSize } = window.__TAURI__.window;

    resizeWindowToContent();

/** 画面サイズを自動調整する */
async function resizeWindowToContent() {

    // コンテンツ全体の高さを取得
    const width  = document.documentElement.scrollWidth;
    const height = document.documentElement.scrollHeight;

    // 現在のウィンドウを取得し、サイズを適用
    const window = getCurrentWindow();
    await window.setSize(new LogicalSize(width, height));
}
