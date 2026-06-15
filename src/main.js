// src/main.ts
var { invoke } = window.__TAURI__.core;
var { listen } = window.__TAURI__.event;
var dialog = window.__TAURI__.dialog;
var { getCurrentWindow, LogicalSize } = window.__TAURI__.window;
var DOM = {
  // 設定の読込中のグレーアウト
  loadingOverlay: document.getElementById("loading-overlay"),
  // バックアップ設定
  sourcePath: document.getElementById("source-path"),
  sourceBtn: document.getElementById("source-btn"),
  destinationPath: document.getElementById("destination-path"),
  destinationBtn: document.getElementById("destination-btn"),
  recursive: document.getElementById("recursive"),
  fileType: document.getElementById("file-type"),
  fileTypeTip: document.getElementById("file-type-tip"),
  extensions: document.getElementById("extensions"),
  // デスクトップ通知の設定
  isNotify: document.getElementById("is-notify"),
  isNotifyUnsaved: document.getElementById("is-notify-unsaved"),
  notifyInterval: document.getElementById("notify-interval"),
  notifyIntervalDiv: document.getElementById("notify-interval-div"),
  // アプリ起動時の設定
  isShown: document.getElementById("is-shown"),
  autoStart: document.getElementById("auto-start"),
  // 操作ボタン
  startBtn: document.getElementById("start-btn"),
  stopBtn: document.getElementById("stop-btn"),
  // ログ表示
  logBox: document.getElementById("log-box")
};
(async () => {
  resizeWindowToContent();
  DOM.startBtn.addEventListener("click", onStartButton);
  DOM.stopBtn.addEventListener("click", onStopButton);
  DOM.sourceBtn.addEventListener("click", () => onFolderSelectButton(DOM.sourcePath));
  DOM.destinationBtn.addEventListener("click", () => onFolderSelectButton(DOM.destinationPath));
  DOM.fileType.addEventListener("change", onFileType);
  DOM.isNotifyUnsaved.addEventListener("change", onIsNotifyUnsaved);
  await initEventListener();
  loadConfig();
})();
async function onFileType() {
  const ByExtensions = DOM.fileType.value == "by-extensions";
  DOM.extensions.hidden = !ByExtensions;
  DOM.fileTypeTip.hidden = !ByExtensions;
}
async function onIsNotifyUnsaved() {
  DOM.notifyIntervalDiv.hidden = !DOM.isNotifyUnsaved.checked;
}
async function onFolderSelectButton(pathInput) {
  try {
    const selected = await dialog.open({
      directory: true,
      // フォルダ選択モード
      multiple: false
      // 複数選択を無効 → 戻り値は配列ではなくstring型
    });
    if (selected)
      pathInput.value = selected;
  } catch (error) {
    console.error("\u30C0\u30A4\u30A2\u30ED\u30B0\u306E\u8D77\u52D5\u306B\u5931\u6557:", error);
  }
}
async function onStartButton() {
  DOM.startBtn.disabled = true;
  const allFilesEnabled = DOM.fileType.value == "all-files-enabled";
  const extensionsArray = DOM.extensions.value.split(/[\s,]/).map((ext) => ext.trim()).filter((ext) => ext.length > 0);
  const notifyInterval = (() => {
    let num = parseInt(DOM.notifyInterval.value, 10) || 0;
    return num < 0 ? 0 : num;
  })();
  const config = {
    sourcePath: DOM.sourcePath.value,
    destinationPath: DOM.destinationPath.value,
    recursive: DOM.recursive.checked,
    allFilesEnabled,
    extensions: extensionsArray,
    isNotify: DOM.isNotify.checked,
    isNotifyUnsaved: DOM.isNotifyUnsaved.checked,
    notifyInterval,
    isShown: DOM.isShown.checked,
    autoStart: DOM.autoStart.checked
  };
  try {
    await invoke("start_watching", { config });
    DOM.stopBtn.disabled = false;
  } catch (error) {
    DOM.startBtn.disabled = false;
    console.log("\u958B\u59CB\u30DC\u30BF\u30F3\u5931\u6557: " + error);
  }
}
async function onStopButton() {
  DOM.stopBtn.disabled = true;
  try {
    await invoke("stop_watching");
    DOM.startBtn.disabled = false;
  } catch (error) {
    DOM.stopBtn.disabled = false;
    console.log("\u505C\u6B62\u30DC\u30BF\u30F3\u5931\u6557: " + error);
  }
}
async function resizeWindowToContent() {
  const width = document.documentElement.scrollWidth;
  const height = document.documentElement.scrollHeight;
  const window2 = getCurrentWindow();
  await window2.setSize(new LogicalSize(width, height));
}
async function loadConfig() {
  let config = await invoke("get_config");
  if (DOM.loadingOverlay)
    DOM.loadingOverlay.style.display = "none";
  document.body.inert = false;
  DOM.startBtn.disabled = false;
  const fileType = config.allFilesEnabled ? "all-files-enabled" : "by-extensions";
  DOM.sourcePath.value = config.sourcePath;
  DOM.destinationPath.value = config.destinationPath;
  DOM.recursive.checked = config.recursive;
  DOM.fileType.value = fileType;
  DOM.extensions.value = config.extensions.join(" ");
  DOM.isNotify.checked = config.isNotify;
  DOM.isNotifyUnsaved.checked = config.isNotifyUnsaved;
  DOM.notifyInterval.value = config.notifyInterval.toString();
  DOM.isShown.checked = config.isShown;
  DOM.autoStart.checked = config.autoStart;
  onFileType();
  onIsNotifyUnsaved();
  if (config.autoStart) onStartButton();
}
async function initEventListener() {
  await listen("log-event", (event) => {
    if (!DOM.logBox) {
      console.log("DOM.logBox \u304C\u3042\u308A\u307E\u305B\u3093");
      return;
    }
    const { level, title, body } = event.payload;
    const date = (/* @__PURE__ */ new Date()).toLocaleTimeString(
      [],
      { hour: "2-digit", minute: "2-digit", second: "2-digit" }
    );
    const logItem = document.createElement("div");
    logItem.classList.add(
      "log-item",
      level
      // レベル名 = CSSクラス名
    );
    logItem.innerHTML = `<span class="log-time">${date}</span><span class="log-title">${title}</span><span class="log-body">${body}</span>`;
    DOM.logBox.insertBefore(logItem, DOM.logBox.firstChild);
    if (DOM.logBox.children.length > 1e3) {
      const lastChild = DOM.logBox.lastChild;
      if (lastChild) DOM.logBox.removeChild(lastChild);
    }
  });
  await listen("start", onStartButton);
  await listen("stop", onStopButton);
}
