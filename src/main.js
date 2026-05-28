
// Tauriの通信用関数
const { invoke } = window.__TAURI__.core;

// 「開始」ボタン
const startButton = document.getElementById("start-btn");
startButton.addEventListener("click", async () => {

    const pathInput = document.getElementById("path").value;
    const extInput = document.getElementById("ext").value;
    const statusEl = document.getElementById("status");
    
    statusEl.innerText = "処理中...";

    try {
        // Rust側の関数を呼び出す
        // 引数名はRust側と一致させる
        const response = await invoke("start_backup", {
            path: pathInput,
            extension: extInput
        });

        // RustからOkが返った場合
        // statusEl.innerText = response;
        // statusEl.style.color = "green";

    } catch (error) {

        // RustからErrが返った場合
        // statusEl.innerText = `エラー: ${error}`;
        // statusEl.style.color = "red";
    }
});


// 現在のウィンドウオブジェクトを取得
const { getCurrentWebviewWindow } = window.__TAURI__.webviewWindow;
const appWindow = getCurrentWebviewWindow();

// 「トレイに格納」ボタン
document.getElementById("hide-btn").addEventListener("click", async () => {
    await appWindow.hide();
});

//=============================================================================
// Rustから受信する
//=============================================================================

// Tauriのイベント用関数 listen
const { listen } = window.__TAURI__.event;

// ページが読み込まれたら、Rustからのイベントを監視する
async function initEventListener() {

    // Rust側のイベント名と合わせる
    await listen("backup-event", (event) => {

        // Rustからの送信されたデータを取り出す
        const log = event.payload;
        // console.log(log);

        // 現在時刻を文字列化 (HH:MM 形式)
        const date = new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

        // ログに表示する
        const statusEl = document.getElementById("status");
        statusEl.innerText = `[${date}] ${log}`;

        // console.log(`状況: ${log.status}, メッセージ: ${log.message}`);
        // statusEl.innerText = `[${new Date().toLocaleTimeString()}] ${log.message}`;
    });
}

// 実行を忘れずに
initEventListener();