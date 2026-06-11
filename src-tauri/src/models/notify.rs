//! 通知関連のデータと機能を定義 (JS・デスクトップ・標準出力へ送信)

use std::sync::Mutex;
use std::sync::Arc;
use std::path::{Path, PathBuf};
use crate::utilities::ResutlErrPrint;
use crate::utilities::lock_mutex;

//=============================================================================
// 通知方法の選択肢
//=============================================================================

/// 通知範囲
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NotifyRange {
    Dialog,     // コンソール + GUIログ + ダイアログ(確実に通知)
    Desktop,    // コンソール + GUIログ + デスクトップ通知(非通知が可能)
    Log,        // コンソール + GUIログ
    Console,    // コンソール (開発用)
    None,       // 通知が不要になったもの
}


/// 通知レベル (GUIログへ表示する文字色)
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")] // JSでは「info, error, remark」になる
pub enum NotifyLevel {
    Error,      // 赤: エラー
    Info,       // 緑: 主要なイベント
    Remark,     // 灰: 微細な情報 (処理スキップなど)
}

//=============================================================================
// 送信用データ型
//=============================================================================

/// UIへの送信用のデータ型
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyPayload {
    pub level: NotifyLevel,   // ユーザーへの通知レベル
    pub range: NotifyRange,   // 通知範囲 (コンソール、ログ、デスクトップ通知、ダイアログ)
    pub title: &'static str,  // デスクトップ通知のタイトル部分 (発生イベントの説明)
    pub body: String,         // デスクトップ通知のボディ部分 (ファイル名など)
}


/// 送信用データ型への変換機能
/// ※これを実装した型は Notifier で送信できる
pub trait ToNotify {

    /// UIへの送信用の型に変換
    fn to_payload(&self) -> NotifyPayload;
}

//=============================================================================
// 送信機 (モック)
//=============================================================================

// #[cfg(not(test))] について
// Windows環境で、テスト時に使っていないReal側のtauri::AppHandleを参照してしまう。
// テストバイナリにはそれが含まれない為、
// テスト起動直後に STATUS_ENTRYPOINT_NOT_FOUND が発生してしまう。

/// 送信機の本番/モックを切替え可能にする
#[derive(Clone)]
pub enum Notifier {
    #[cfg(not(test))]
    Real(AppNotifier),
    Mock(MockNotifier),
}

impl Notifier {
    pub fn notify(&self, event: &impl ToNotify) {
        match self {
            #[cfg(not(test))]
            Self::Real(notifier) => notifier.notify(event),
            Self::Mock(notifier) => notifier.notify(event),
        }
    }
}

//=============================================================================
// 送信機 (テスト用実装)
//=============================================================================

/// テスト用送信機 (AppHandleが不要)
#[derive(Debug, Clone)]
pub struct MockNotifier {
    pub log: Arc<Mutex< Vec<NotifyPayload> >>,
}

impl MockNotifier {
    pub fn new() -> Self {
        Self { log: Arc::new(Mutex::new(Vec::new())) }
    }

    /// 送信用型に変換し、Vecフィールドに追加する
    pub fn notify(&self, event: &impl ToNotify) {
        let mut log = lock_mutex(&self.log);
        log.push(event.to_payload());
    }
}

//=============================================================================
// 送信機 (実装)
//=============================================================================

/// UIへの送信機
#[derive(Debug, Clone)]
pub struct AppNotifier {
    app: tauri::AppHandle,      // JSへの送信に必要
    is_desktop_notify: bool,    // デスクトップ通知を行うか
}

impl AppNotifier {
    
    pub fn new(app: &tauri::AppHandle, is_desktop_notify: bool) -> Self {
        Self {
            app: app.clone(),
            is_desktop_notify,
        }
    }


    /// UIへ情報を送信する
    pub fn notify(&self, event: &impl ToNotify) {

        // 送信用のデータ型に変換
        let payload = event.to_payload();

        // 設定された範囲へ通知
        use NotifyRange::*;
        match payload.range {

            Dialog => {
                self.consle(&payload);
                self.log(&payload);
                self.dialog(&payload);
            }

            Desktop => {
                self.consle(&payload);
                self.log(&payload);
                self.desktop(payload);
            }

            Log => {
                self.consle(&payload);
                self.log(&payload);
            }

            Console => {
                self.consle(&payload);
            }

            None => {}
        }
    }


    /// デバッグ時のみコンソールへ表示
    fn consle(&self, payload: &NotifyPayload) {
        if cfg!(debug_assertions) {
            println!("{}: {}", payload.title, payload.body);
        }
    }


    /// ログウィンドウ(JS側) へ送信
    fn log(&self, payload: &NotifyPayload) {
        use tauri::Emitter;

        // TauriのイベントシステムでJSへ送信
        // 第一引数: イベント名(自由)
        // 第二引数: 送信するデータ
        self.app.emit("log-event", payload).eprint("JSへのログ通知に失敗");
    }


    /// モーダルダイアログを表示
    fn dialog(&self, payload: &NotifyPayload) {
        use tauri::Manager;
        use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

        // ダイアログ用メッセージを作成
        let message = format!("{}:\n{}", payload.title, payload.body);

        // メインウィンドウを取得
        let main_window = self.app.get_webview_window("main")
            .expect("メインウィンドウの取得に失敗");

        // モーダルダイアログを表示
        self.app.dialog()
            .message(message)
            // .title("タイトル")   // アプリ名を表示するため、指定しない
            .kind(MessageDialogKind::Info)
            .buttons(MessageDialogButtons::Ok)
            .parent(&main_window)   // メインウィンドウをブロック
            .blocking_show();
    }


    /// デスクトップ通知をする
    /// ※インストーラを使用しない場合、アプリ名がPowerShellになる
    fn desktop(&self, payload: NotifyPayload) {
        use tauri_plugin_notification::NotificationExt;

        if !self.is_desktop_notify { return; }

        // move用コピー
        let app = self.app.clone();

        // メインスレッドから通知しないと、Windowsは1度保留にしてしまう
        self.app.run_on_main_thread(move || {

            // デスクトップ通知を送信
            app.notification()
                .builder()
                .title(payload.title)
                .body(payload.body)
                .show()
                .eprint("デスクトップ通知に失敗");

        }).eprint("メインスレッドへの委託に失敗");
    }
}

//=============================================================================
// timer.rs の通知型
//=============================================================================

/// 「ファイル未保存時間」計測時の情報
#[derive(Debug, Clone, serde::Serialize)]
pub enum TimerInfo {
    Elapsed { minutes: u64 },   // 未保存時間が指定分経過した
    // Reseted,                 // 未保存時間をリセットした
    // Started,                 // 計測スレッドを開始
    // Stopped,                 // 計測スレッドが終了
}

impl ToNotify for TimerInfo {
    fn to_payload(&self) -> NotifyPayload {
        use NotifyLevel::*;
        use NotifyRange::*;
        use TimerInfo::*;

        let (level, range, title, body) = match self {
            Elapsed{minutes} => (Info, Desktop, "ファイル未保存期間", format!("{minutes}分経過")),
        };

        NotifyPayload { level, range, title, body }
    }
}

//=============================================================================
// config.rs の戻り値型
//=============================================================================

/// Config型のバリデーションチェックの結果値
#[derive(Debug, Clone, serde::Serialize)]
pub enum ConfigError {
    InvalidSourcePath,          // バックアップ元フォルダが無効
    InvalidDestinationPath,     // バックアップ先フォルダが無効
    PathConflict,               // バックアップ元/先 が同じ
    NoExtension,                // 拡張子(バックアップ対象) が1つも設定されていない
    InvalidNotifyInterval,      // ファイルの未保存を通知の時間設定が無効
}

impl ToNotify for ConfigError {
    fn to_payload(&self) -> NotifyPayload {
        use ConfigError::*;

        let level = NotifyLevel::Error;
        let range = NotifyRange::Dialog;    // ダイアログで確実に通知
        let title = "開始失敗";

        let body = match self {
            InvalidSourcePath      => "保存を検知するフォルダが無効です".into(),
            InvalidDestinationPath => "バックアップ先フォルダが無効です".into(),
            PathConflict           => "保存検知フォルダとバックアップ先が同じです".into(),
            NoExtension            => "拡張子を一つ以上設定してください".into(),
            InvalidNotifyInterval  => "未保存通知は1分以上に設定してください".into(),
        };

        NotifyPayload { level, range, title, body }
    }
}

//=============================================================================
// watch.rs のイベント情報型 / 戻り値型
//=============================================================================

/// フォルダ監視中のイベント情報 (UI/ログへの送信用)
#[derive(Debug, Clone, serde::Serialize)]
pub enum WatchInfo {
    Detected(PathBuf),      // ファイル変更の検出
    NotTarget(PathBuf),     // バックアップの対象外のためスキップ
    DebounceError,          // DebounceEventResult のエラー
}

impl ToNotify for WatchInfo {
    fn to_payload(&self) -> NotifyPayload {
        use NotifyLevel::*;
        use NotifyRange::*;
        use WatchInfo::*;

        let (level, range, title, body) = match self {
            Detected(path)  => (Remark, Console, "変更を検出", get_filename(path)),
            NotTarget(path) => (Remark, Console, "対象外のためスキップ", get_filename(path)),
            DebounceError   => (Error,  Desktop, "フォルダ監視中のエラー", "デバウンスエラーが発生".into()),
        };

        NotifyPayload { level, range, title, body }
    }
}


/// フォルダ監視の開始処理の結果 (UI/ログへの送信用)
#[derive(Debug, Clone, serde::Serialize)]
pub enum StartResult {
    Success,                // フォルダ監視の開始に成功
    AlreadyRunning,         // 既に監視が開始している
    NewDebouncerFailed,     // デバウンサーの生成に失敗
    DebounceStartFailed,    // デバウンサーが監視開始に失敗
}

impl ToNotify for StartResult {
    fn to_payload(&self) -> NotifyPayload {
        use NotifyLevel::*;
        use StartResult::*;

        let range = NotifyRange::Desktop;
        let (level, title, body) = match self {
            Success             => (Info,  "自動バックアップを開始しました", "".into()),
            AlreadyRunning      => (Info,  "既に開始済みです", "".into()),
            NewDebouncerFailed  => (Error, "開始失敗", "デバウンサーの生成に失敗しました".into()),
            DebounceStartFailed => (Error, "開始失敗", "デバウンサーの開始に失敗しました".into()),
        };

        NotifyPayload { level, range, title, body }
    }
}


/// フォルダ監視の停止処理の結果 (UI/ログへの送信用)
#[derive(Debug, Clone, serde::Serialize)]
pub enum StopResult {
    Success,        // フォルダ監視を終了
    AlreadyStopped, // 既に停止中
}

impl ToNotify for StopResult {
    fn to_payload(&self) -> NotifyPayload {
        use StopResult::*;

        let level = NotifyLevel::Info;
        let range = NotifyRange::Desktop;
        let (title, body) = match self {
            Success        => ("自動バックアップを停止しました", "".into()),
            AlreadyStopped => ("既に停止済みです", "".into()),
        };

        NotifyPayload { level, range, title, body }
    }
}

//=============================================================================
// wait.rs の戻り値型
//=============================================================================

/// ファイル書込終了待ちの結果 (UI/ログへの送信用)
#[derive(Debug, Clone, serde::Serialize)]
pub enum WaitResult {
    Success,          // 書込終了を確認した
    Locked(PathBuf),  // ファイルがロックされている
    Missing(PathBuf), // ファイルを見失った
}

impl ToNotify for WaitResult {
    fn to_payload(&self) -> NotifyPayload {
        use NotifyLevel::*;
        use NotifyRange::*;
        use WaitResult::*;

        let (level, range, title, body) = match self {
            Success       => (Remark, None,    "ファイルの書込み終了を検知", "".into()),
            Locked(path)  => (Error,  Desktop, "ファイルがロック中", get_filename(path)),
            Missing(path) => (Error,  Desktop, "ファイル消失または読取権限なし", get_filename(path)),
        };

        NotifyPayload { level, range, title, body }
    }
}

//=============================================================================
// backup.rs の戻り値型
//=============================================================================

// バックアップ処理の結果 (UI/ログへの送信用)
#[derive(Debug, Clone, serde::Serialize)]
pub enum BackupResult {
    Copied(PathBuf),
    AlreadyExists(PathBuf),
    InvalidFileName(PathBuf),
    MetadataFailed(PathBuf),
    ModifiedFailed(PathBuf),
    CopyFailed(PathBuf),
}

impl ToNotify for BackupResult {
    fn to_payload(&self) -> NotifyPayload {
        use BackupResult::*;
        use NotifyLevel::*;
        use NotifyRange::*;

        let (level, range, title, body) = match self {
            Copied         (path) => (Info,  Desktop, "バックアップ", get_filename(path)),
            AlreadyExists  (path) => (Remark, Log,    "既にバックアップ済み", get_filename(path)),
            InvalidFileName(path) => (Error, Desktop, "ファイル名の取得に失敗", get_filename(path)),
            MetadataFailed (path) => (Error, Desktop, "ファイル情報の取得に失敗", get_filename(path)),
            ModifiedFailed (path) => (Error, Desktop, "最終更新時の取得に失敗", get_filename(path)),
            CopyFailed     (path) => (Error, Desktop, "コピー失敗", get_filename(path)),
        };

        NotifyPayload { level, range, title, body }
    }
}

//=============================================================================
// ユーティリティ
//=============================================================================

// ※ フォルダに対して使用
/// 拡張接頭辞を外す (UI表示用)
/// PathBuf::canonicalize() すると「\\?\」が付与されるため、表示用に削除する
fn _clean_path(path: &Path) -> String {
    
    // PathBuf → String 変換
    // ※UTF-8として不正な部分は「」に変換する
    let path_str = path.to_string_lossy();

    // Windows環境のみ拡張接頭辞を外す
    #[cfg(windows)]
    if path_str.starts_with(r"\\?\") {
        return path_str[4..].to_string();
    }

    path_str.into_owned()
}

// ※ ファイルに対して使用
/// ファイル名のみを抽出
fn get_filename(path: &Path) -> String {
    path.file_name()
        .and_then(|os_str| os_str.to_str())
        .unwrap_or("ファイル名が不明")
        .to_string()
}

// ※ 現在不使用
/// 相対パス名のみ抽出 (エラー時はそのまま返す)
fn _get_relative_path(path: &Path, base: &Path) -> PathBuf {
    path.strip_prefix(base).unwrap_or(path).to_path_buf()
}
