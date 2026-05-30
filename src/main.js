//=============================================================================
// グローバル変数
//=============================================================================

// Tauriの通信用関数
const { invoke } = window.__TAURI__.core;

// Tauriのイベント用関数 listen
const { listen } = window.__TAURI__.event;

// 現在のウィンドウオブジェクトを取得
const { getCurrentWebviewWindow } = window.__TAURI__.webviewWindow;
const appWindow = getCurrentWebviewWindow();

/** getElementById のリスト */
const DOM = {
    // 設定の読込中のグレーアウト
    loadingOverlay  : document.getElementById("loading-overlay"),

    // Config構造体と対応する <input>
    sourcePath      : document.getElementById("source-path"),
    destinationPath : document.getElementById("destination-path"),
    extensions      : document.getElementById("extensions"),
    isShown         : document.getElementById("is-shown"),
    isNotify        : document.getElementById("is-notify"),

    // 操作ボタン
    startBtn        : document.getElementById("start-btn"),
    stopBtn         : document.getElementById("stop-btn"),

    // 状態/ログを表示
    // log             : document.getElementById("log"),
    logBox          : document.getElementById("log-box"),
};

//=============================================================================
// 起動時に実行する処理
//=============================================================================

(async () => {
    // 各ボタンにイベントリスナーを設定
    DOM.startBtn.addEventListener("click", onStartButton);
    DOM.stopBtn.addEventListener("click", onStopButton);

    initEventListener();
    loadConfig();
})();

//=============================================================================
// 関数
//=============================================================================

/** 設定ファイルを読み込む */
async function loadConfig() {

    // Rust側関数を実行
    let config = await invoke("get_config");

    // ローディングオーバレイを消す
    if (DOM.loadingOverlay)
        DOM.loadingOverlay.style.display = "none";

    // 無効化を解除
    document.body.inert = false;
    DOM.startBtn.disabled = false;

    // HTMLへ値を反映
    // config内はキャメルケース化済み
    DOM.sourcePath.value      = config.sourcePath;
    DOM.destinationPath.value = config.destinationPath;
    DOM.extensions.value      = config.extensions.join(" "); // 配列→文字列
    DOM.isShown.checked       = config.isShown;
    DOM.isNotify.checked      = config.isNotify;
}


/** 「開始」ボタンが押されたとき */
async function onStartButton() {

    // 処理開始の表示
    // DOM.log.innerText = "開始処理中...";
    DOM.startBtn.disabled = true;

    // 拡張子を取得し、文字列→配列へ変換
    const extensionsArray = DOM.extensions.value
        .split(/[\s,]/)                 // スペース(連続含む) or カンマ で分割
        .map(ext => ext.trim())         // 前後の空白を削除
        .filter(ext => ext.length > 0); // 空文字は除外

    // HTMLの値を取得、Config構造体に合わせて格納
    const config = {
        sourcePath      : DOM.sourcePath.value,
        destinationPath : DOM.destinationPath.value,
        extensions      : extensionsArray,
        isShown         : DOM.isShown.checked,
        isNotify        : DOM.isNotify.checked,
    };

    try {
        // Rust側の関数を呼び出す
        // 引数名はRust側と一致させる ※ オブジェクトで指定
        const response = await invoke("start_watching", {config: config});
        DOM.stopBtn.disabled = false;

        
        // const response = await invoke("start_watching", {
        //     path: pathInput,
        //     extension: extInput
        // });

        // RustからOkが返った場合
        // statusEl.innerText = response;
        // statusEl.style.color = "green";

    } catch (error) {
        DOM.startBtn.disabled = false;
        console.log("開始ボタン失敗: "+ error);

        // RustからErrが返った場合
        // statusEl.innerText = `エラー: ${error}`;
        // statusEl.style.color = "red";
    }
}


/** 「停止」ボタンが押されたとき */
async function onStopButton() {

    // 処理開始の表示
    // DOM.log.innerText = "停止処理中...";
    DOM.stopBtn.disabled = true;

    try {
        // Rust側の関数を呼び出す
        // 引数名はRust側と一致させる ※ オブジェクトで指定
        await invoke("stop_watching");
        DOM.startBtn.disabled = false;

        // RustからOkが返った場合
        // statusEl.innerText = response;
        // statusEl.style.color = "green";

    } catch (error) {
        DOM.stopBtn.disabled = false;
        console.log("停止ボタン失敗: "+ error);
        // RustからErrが返った場合
        // statusEl.innerText = `エラー: ${error}`;
        // statusEl.style.color = "red";
    }
}


// const startButton = document.getElementById("start-btn");
// startButton.addEventListener("click", async () => {

//     const pathInput = document.getElementById("source").value;
//     // const extInput = document.getElementById("ext").value;
//     const statusEl = document.getElementById("log");
    
//     statusEl.innerText = "処理中...";

//     try {
//         // Rust側の関数を呼び出す
//         // 引数名はRust側と一致させる
//         const response = await invoke("start_watching");

//         // const response = await invoke("start_watching", {
//         //     path: pathInput,
//         //     extension: extInput
//         // });

//         // RustからOkが返った場合
//         // statusEl.innerText = response;
//         // statusEl.style.color = "green";

//     } catch (error) {

//         // RustからErrが返った場合
//         // statusEl.innerText = `エラー: ${error}`;
//         // statusEl.style.color = "red";
//     }
// });


// 「トレイに格納」ボタンが押されたとき
// document.getElementById("hide-btn").addEventListener("click", async () => {
//     await appWindow.hide();
// });

//=============================================================================
// Rustから受信する
//=============================================================================

/** Rustからのイベントを監視する */
async function initEventListener() {

    // Rust側のイベント名と合わせる
    await listen("log-event", (event) => {

        // Rustからの送信されたデータを取り出す
        const dto = event.payload;
        console.log(dto);
        const {level, title, body} = dto;

        // 現在時刻を文字列化 (HH:MM 形式)
        const date = new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit"});

        // const now = new Date();
        // const timeStr = `[${now.getHours().toString().padStart(2, '0')}:${now.getMinutes().toString().padStart(2, '0')}:${now.getSeconds().toString().padStart(2, '0')}]`;

        // ログに表示する
        // DOM.log.innerText = `[${date}] ${title}`;


        if (!DOM.logBox) return;

        // 新しいログ要素を作成 (p or div)
        const logItem = document.createElement("div");

        // ログの種類に応じてCSSを適用
        // const type = "error";
        // logItem.classList.add("log-item", `log-${level}`);
        logItem.classList.add("log-item", `log-${level}`);

        // console.log(logItem);
        // console.log(logItem.textContent);

        // テキストを設定 (タイムスタンプ + メッセージ)
        // logItem.textContent = `[${date}] ${log}`;
        logItem.innerHTML = `
            <span class="log-time">${date}</span>
            <span class="log-title log-${level}">${title}</span>
            <span class="log-body">${body}</span>
        `;

        // ボックスの先頭に追加
        DOM.logBox.insertBefore(logItem, DOM.logBox.firstChild);

        // 1000件を超えたら古いログを削除
        if (DOM.logBox.children.length > 1000) {
            DOM.logBox.removeChild(DOM.logBox.lastChild);
        }
        // console.log(`状況: ${log.log}, メッセージ: ${log.message}`);
        // statusEl.innerText = `[${new Date().toLocaleTimeString()}] ${log.message}`;
    });
}

// /** Rustからのイベントを監視する */
// async function initEventListener() {

//     // Rust側のイベント名と合わせる
//     await listen("log-event", (event) => {

//         // Rustからの送信されたデータを取り出す
//         const log = event.payload;
//         // console.log(log);

//         // 現在時刻を文字列化 (HH:MM 形式)
//         const date = new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

//         // ログに表示する
//         // const statusEl = document.getElementById("log");
//         DOM.log.innerText = `[${date}] ${log}`;

//         // console.log(`状況: ${log.log}, メッセージ: ${log.message}`);
//         // statusEl.innerText = `[${new Date().toLocaleTimeString()}] ${log.message}`;
//     });
// }


