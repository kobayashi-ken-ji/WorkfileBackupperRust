// ts-rsで出力された型定義ファイル
import {Config, NotifyPayload} from "./types.ts";

//=============================================================================
// グローバル変数
//=============================================================================

// Tauriの関数 (global.d.tsで型を定義)
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const dialog = window.__TAURI__.dialog;
const { getCurrentWindow, LogicalSize } = window.__TAURI__.window;

/** ドキュメントの要素リスト */
const DOM = {

    // 設定の読込中のグレーアウト
    loadingOverlay  : document.getElementById("loading-overlay") as HTMLElement,

    // バックアップ設定
    sourcePath      : document.getElementById("source-path") as HTMLInputElement,
    sourceBtn       : document.getElementById("source-btn") as HTMLButtonElement,
    destinationPath : document.getElementById("destination-path") as HTMLInputElement,
    destinationBtn  : document.getElementById("destination-btn") as HTMLButtonElement,
    recursive       : document.getElementById("recursive") as HTMLInputElement,
    fileType        : document.getElementById("file-type") as HTMLInputElement,
    fileTypeTip     : document.getElementById("file-type-tip") as HTMLElement,
    extensions      : document.getElementById("extensions") as HTMLInputElement,

    // デスクトップ通知の設定
    isNotify          : document.getElementById("is-notify") as HTMLInputElement,
    isNotifyUnsaved   : document.getElementById("is-notify-unsaved") as HTMLInputElement,
    notifyInterval    : document.getElementById("notify-interval") as HTMLInputElement,
    notifyIntervalDiv : document.getElementById("notify-interval-div") as HTMLElement,

    // アプリ起動時の設定
    isShown   : document.getElementById("is-shown") as HTMLInputElement,
    autoStart : document.getElementById("auto-start") as HTMLInputElement,

    // 操作ボタン
    startBtn  : document.getElementById("start-btn") as HTMLButtonElement,
    stopBtn   : document.getElementById("stop-btn") as HTMLButtonElement,

    // ログ表示
    logBox    : document.getElementById("log-box") as HTMLElement,
};

//=============================================================================
// 起動時の処理を実行
//=============================================================================

(async () => {
    resizeWindowToContent();

    // 各ボタンにイベントリスナーを設定
    DOM.startBtn.addEventListener("click", onStartButton);
    DOM.stopBtn.addEventListener("click", onStopButton);
    DOM.sourceBtn.addEventListener("click", ()=>onFolderSelectButton(DOM.sourcePath));
    DOM.destinationBtn.addEventListener("click", ()=>onFolderSelectButton(DOM.destinationPath));
    DOM.fileType.addEventListener("change", onFileType);
    DOM.isNotifyUnsaved.addEventListener("change", onIsNotifyUnsaved);

    await initEventListener();
    loadConfig();
})();

//=============================================================================
// ボタンが押されたときの処理
//=============================================================================

/** 「拡張子の入力欄」の可視/不可視を切替え */
async function onFileType() {
    const ByExtensions = (DOM.fileType.value == "by-extensions");
    DOM.extensions.hidden = !ByExtensions;
    DOM.fileTypeTip.hidden = !ByExtensions;
}


/** 「ファイル未保存の通知」の可視/不可視を切替え */
async function onIsNotifyUnsaved() {
    DOM.notifyIntervalDiv.hidden = !DOM.isNotifyUnsaved.checked;
}


/** 
 * フォルダ選択ボタンが押されたとき
 * @param pathInput 入力ボックス(選択されたフォルダパスの反映先)
 */
async function onFolderSelectButton(pathInput: HTMLInputElement) {

    try {
        // フォルダ選択ダイアログを開く
        const selected = await dialog.open({
            directory: true,    // フォルダ選択モード
            multiple: false,    // 複数選択を無効 → 戻り値は配列ではなくstring型
        });

        // null以外 → 入力ボックスへ反映
        if (selected)
            pathInput.value = selected as string;

    } catch(error) {
        console.error("ダイアログの起動に失敗:", error);
    }
}


/** 「開始」ボタンが押されたとき */
async function onStartButton() {

    // 開始ボタン無効化
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
    const config: Config = {
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
        await invoke("start_watching", {config: config});
        
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

    // 停止ボタン無効化
    DOM.stopBtn.disabled = true;

    try {
        await invoke("stop_watching");
        DOM.startBtn.disabled = false;

    } catch (error) {
        DOM.stopBtn.disabled = false;
        console.log("停止ボタン失敗: "+ error);
    }
}

//=============================================================================
// 起動時の処理
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


/** 設定ファイルを読み込む */
async function loadConfig() {

    // Rust側関数を実行
    let config = await invoke<Config>("get_config");

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
    DOM.notifyInterval.value    = config.notifyInterval.toString();
    DOM.isShown.checked         = config.isShown;
    DOM.autoStart.checked       = config.autoStart;

    // 表示/非表示を反映
    onFileType();
    onIsNotifyUnsaved();

    // 自動開始が設定されていれば、起動時に開始
    if (config.autoStart) onStartButton();
}


/** 「Rust側でemitした時」に実行する処理を登録 */
async function initEventListener() {

    // Rust側のイベント名と合わせる
    await listen<NotifyPayload>("log-event", (event) => {

        if (!DOM.logBox) { 
            console.log("DOM.logBox がありません");
            return;
        }

        // Rustからの送信されたデータを取り出す
        const {level, title, body} = event.payload;

        // 現在時刻を文字列化 (HH:MM 形式)
        const date = new Date().toLocaleTimeString([],
            { hour: "2-digit", minute: "2-digit", second: "2-digit" });

        // 新しいログ要素を作成 (p or div)
        const logItem = document.createElement("div");

        // ログの種類に応じてCSSを適用
        logItem.classList.add(
            "log-item",
            level  // レベル名 = CSSクラス名
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
            const lastChild = DOM.logBox.lastChild;
            if (lastChild) DOM.logBox.removeChild(lastChild);
        }
    });


    // フォルダ監視の 開始/終了
    await listen("start", onStartButton);
    await listen("stop" , onStopButton);
}
