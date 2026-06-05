//=============================================================================
// グローバル変数
//=============================================================================

// Tauriの通信用関数
const { invoke } = window.__TAURI__.core;

// Tauriのイベント用関数 listen
const { listen } = window.__TAURI__.event;

const { open } = window.__TAURI__.dialog;
const { getCurrentWindow, LogicalSize } = window.__TAURI__.window;

// 現在のウィンドウオブジェクトを取得
const { getCurrentWebviewWindow } = window.__TAURI__.webviewWindow;
const appWindow = getCurrentWebviewWindow();

/** getElementById のリスト */
const DOM = {
    // 設定の読込中のグレーアウト
    loadingOverlay  : document.getElementById("loading-overlay"),

    // バックアップ設定
    sourcePath      : document.getElementById("source-path"),
    sourceBtn       : document.getElementById("source-btn"),
    destinationPath : document.getElementById("destination-path"),
    destinationBtn  : document.getElementById("destination-btn"),
    recursive       : document.getElementById("recursive"),
    fileType        : document.getElementById("file-type"),
    fileTypeTip     : document.getElementById("file-type-tip"),
    extensions      : document.getElementById("extensions"),

    // デスクトップ通知の設定
    isNotify          : document.getElementById("is-notify"),
    isNotifyUnsaved   : document.getElementById("is-notify-unsaved"),
    notifyInterval    : document.getElementById("notify-interval"),
    notifyIntervalDiv : document.getElementById("notify-interval-div"),

    // アプリ起動時の設定
    isShown   : document.getElementById("is-shown"),
    autoStart : document.getElementById("auto-start"),

    // 操作ボタン
    startBtn  : document.getElementById("start-btn"),
    stopBtn   : document.getElementById("stop-btn"),

    // ログ表示
    logBox    : document.getElementById("log-box"),
};

//=============================================================================
// 起動時に実行する処理
//=============================================================================

(async () => {
    resizeWindowToContent();

    // 各ボタンにイベントリスナーを設定
    DOM.startBtn.addEventListener("click", onStartButton);
    DOM.stopBtn.addEventListener("click", onStopButton);
    DOM.sourceBtn.addEventListener("click", ()=>onFolderSelectButton(DOM.sourcePath, DOM.sourceBtn));
    DOM.destinationBtn.addEventListener("click", ()=>onFolderSelectButton(DOM.destinationPath, DOM.destinationBtn));
    DOM.fileType.addEventListener("change", onFileType);
    DOM.isNotifyUnsaved.addEventListener("change", onIsNotifyUnsaved);
    await initEventListener();
    loadConfig();
})();

//=============================================================================
// 関数
//=============================================================================

/** 画面サイズを自動調整する */
async function resizeWindowToContent() {

    // コンテンツ全体の高さを取得
    const width  = document.documentElement.scrollWidth;
    const height = document.documentElement.scrollHeight;

    // 現在のウィンドウを取得し、サイズを適用
    const window = getCurrentWindow();
    await window.setSize(new LogicalSize(width, height));
}


/** 拡張子入力の可視/不可視を切替え */
async function onFileType() {
    const ByExtensions = (DOM.fileType.value == "by-extensions");

    // // プルダウン部分
    // (ByExtensions)
    //     ? DOM.extensions.classList.remove("invisible")
    //     : DOM.extensions.classList.add("invisible");

    // // ヒント部分
    // (ByExtensions)
    //     ? DOM.fileTypeTip.classList.remove("invisible")
    //     : DOM.fileTypeTip.classList.add("invisible");

    // DOM.extensions.disabled = !ByExtensions;
    DOM.extensions.hidden = !ByExtensions;
    DOM.fileTypeTip.hidden = !ByExtensions;

}


/** 「ファイル未保存の通知」の可視/不可視を切替え */
async function onIsNotifyUnsaved() {

    // 時間指定部分を切替え
    // (DOM.isNotifyUnsaved.checked)
    //     ? DOM.notifyIntervalDiv.classList.remove("invisible")
    //     : DOM.notifyIntervalDiv.classList.add("invisible");

    DOM.notifyIntervalDiv.hidden = !DOM.isNotifyUnsaved.checked;
}


/** フォルダ選択ボタンが押されたとき */
async function onFolderSelectButton(pathInput, selectButton) {

    console.log(pathInput);
    console.log(selectButton);

    try {
        // フォルダ選択ダイアログを開く
        const selected = await open({
            directory: true,    // フォルダ選択モード
            multiple: false,    // 複数選択は無効
        });

        // null以外 → 入力ボックスへ反映
        if (selected)
            pathInput.value = selected;

    } catch(error) {
        console.error("ダイアログの起動に失敗:", error);
    }
}


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

    // config真偽値 → select.valueの文字列に変換
    const fileType = (config.allFilesEnabled)
        ? "all-files-enabled"
        : "by-extensions";

    // HTMLへ値を反映
    // config内はキャメルケース化済み
    DOM.sourcePath.value        = config.sourcePath;
    DOM.destinationPath.value   = config.destinationPath;
    DOM.recursive.checked       = config.recursive;
    DOM.fileType.value          = fileType;
    DOM.extensions.value        = config.extensions.join(" "); // 配列→文字列
    DOM.isNotify.checked        = config.isNotify;
    DOM.isNotifyUnsaved.checked = config.isNotifyUnsaved;
    DOM.notifyInterval.value    = config.notifyInterval;
    DOM.isShown.checked         = config.isShown;
    DOM.autoStart.checked       = config.autoStart;

    // 表示/非表示を反映
    onFileType();
    onIsNotifyUnsaved();

    // 自動開始設定の処理
    if (config.autoStart) onStartButton();
}


/** 「開始」ボタンが押されたとき */
async function onStartButton() {

    // 処理開始の表示
    // DOM.log.innerText = "開始処理中...";
    DOM.startBtn.disabled = true;
    
    // プルダウンの値を取得
    const allFilesEnabled = (DOM.fileType.value == "all-files-enabled");

    // 拡張子を取得し、文字列→配列へ変換
    const extensionsArray = DOM.extensions.value
        .split(/[\s,]/)                 // スペース(連続含む) or カンマ で分割
        .map(ext => ext.trim())         // 前後の空白を削除
        .filter(ext => ext.length > 0); // 空文字は除外

    // 未保存の通知間隔
    const notifyInterval = (()=>{

        // 文字列 → 整数値 (NoNは0へ)
        let num = parseInt(DOM.notifyInterval.value, 10) || 0;

        // 0未満を排除 (Rust側のu64型に合わせる)
        return (num < 0) ? 0 : num;
    })();

    // HTMLの値を取得、Config構造体に合わせて格納
    const config = {
        sourcePath      : DOM.sourcePath.value,
        destinationPath : DOM.destinationPath.value,
        recursive       : DOM.recursive.checked,
        allFilesEnabled : allFilesEnabled,
        extensions      : extensionsArray,
        isNotify        : DOM.isNotify.checked,
        isNotifyUnsaved : DOM.isNotifyUnsaved.checked,
        notifyInterval  : notifyInterval,
        isShown         : DOM.isShown.checked,
        autoStart       : DOM.autoStart.checked,
    };

    try {
        // Rust側の関数を呼び出す
        // 引数名はRust側と一致させる ※ オブジェクトで指定
        const response = await invoke("start_watching", {config: config});
        
        // RustからOkが返った場合
        DOM.stopBtn.disabled = false;

    } catch (error) {

        // RustからErrが返った場合
        DOM.startBtn.disabled = false;
        console.log("開始ボタン失敗: "+ error);
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

/** Rustからのイベントのリスナーを登録する */
async function initEventListener() {

    // Rust側のイベント名と合わせる
    await listen("log-event", (event) => {

        if (!DOM.logBox) { 
            console.log("DOM.logBox がありません");
            return;
        }

        // Rustからの送信されたデータを取り出す
        const dto = event.payload;
        const {level, title, body} = dto;

        // 現在時刻を文字列化 (HH:MM 形式)
        const date = new Date().toLocaleTimeString([],
            { hour: "2-digit", minute: "2-digit", second: "2-digit" });

        // 新しいログ要素を作成 (p or div)
        const logItem = document.createElement("div");

        // ログの種類に応じてCSSを適用
        logItem.classList.add(
            "log-item",
            (level == "errorSilent") ? "error" : level  // レベル名 = CSSクラス名
        );

        // テキストを設定 (タイムスタンプ + メッセージ)
        // 改行やスペースが入ると、2px程度の隙間になるため、+で繋ぐ
        logItem.innerHTML =
            `<span class="log-time">${date}</span>` +
            `<span class="log-title">${title}</span>` +
            `<span class="log-body">${body}</span>`;

        // ボックスの先頭に追加
        DOM.logBox.insertBefore(logItem, DOM.logBox.firstChild);

        // 1000件を超えたら古いログを削除
        if (DOM.logBox.children.length > 1000) {
            DOM.logBox.removeChild(DOM.logBox.lastChild);
        }
    });


    // フォルダ監視の 開始/終了
    await listen("start", onStartButton);
    await listen("stop" , onStopButton);
}
