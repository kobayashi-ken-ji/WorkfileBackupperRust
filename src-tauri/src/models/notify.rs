use std::path::{Path, PathBuf};
use crate::utilities::ResutlErrPrint;

//=============================================================================

// mod notification {
//     use super::*;

//     //=============================================================================
//     // 送信用データ型
//     //=============================================================================

//     /// UIへのデータ送信用の型
//     #[derive(Debug, Clone, serde::Serialize)]
//     #[serde(rename_all = "camelCase")]
//     pub struct MessageDTO {
//         pub level: NotifyLevel,  // ユーザーへの通知レベル
//         pub title: &'static str, // デスクトップ通知のタイトル部分 (発生イベントの説明)
//         pub body: String,        // デスクトップ通知のボディ部分 (ファイル名など)
//     }
    

//     /// DTOへの変換機能
//     /// 各関数の戻り値型に実装する
//     pub trait Message {
//         /// UIへの送信用の型に変換
//         fn to_dto(&self) -> MessageDTO;
//     }

//     //=============================================================================
//     // 送信機
//     //=============================================================================

//     /// 送信機能
//     pub trait Sender {

//         /// UIへ情報を送信する
//         fn send(&self, message: impl Message);
//     }


//     /// テスト用送信機 (AppHandleが不要)
//     pub struct MockSender {}
//     impl Sender for MockSender {
//         fn send(&self, message: impl Message) {

//             let dto = message.to_dto();
//             println!("{}: {}", dto.title, dto.body);
//         }
//     }


//     /// UIへの送信機
//     #[derive(Debug, Clone)]
//     pub struct MessageSender {
//         app: tauri::AppHandle,
//         is_desktop_notify: bool,
//     }

//     impl Sender for MessageSender {
//         fn send(&self, message: impl Message) {

//             use tauri::Emitter;
//             use tauri_plugin_notification::NotificationExt;

//             let dto = message.to_dto();

//             // デバッグ時のみコンソールへ表示
//             if cfg!(debug_assertions) {
//                 println!("{}: {}", dto.title, dto.body);
//             }

//             // Debug → コンソール以外には表示しない
//             if dto.level == NotifyLevel::Debug { return; }

//             // TauriのイベントシステムでJSへ送信
//             // 第一引数: イベント名(自由)
//             // 第二引数: 送信するデータ
//             self.app.emit("log-event", &dto).eprint("JSへのログ通知に失敗");

//             // デスクトップ通知を行うか判定
//             if  !self.is_desktop_notify ||
//                 dto.level == NotifyLevel::Silent ||
//                 dto.level == NotifyLevel::ErrorSilent {
//                 return;
//             }

//             // move用コピー
//             let app_handle_clone = self.app.clone();

//             // メインスレッドから通知しないと、Windowsは1度保留にしてしまう
//             self.app.run_on_main_thread(move || {

//                 // デスクトップ通知を送信
//                 // ※ インストーラを使用しない場合、アプリ名がPowerShellになる
//                 app_handle_clone.notification()
//                     .builder()
//                     .title(dto.title)
//                     .body(dto.body)
//                     .show()
//                     .eprint("デスクトップ通知に失敗");

//             }).eprint("メインスレッドへの委託に失敗");
//         }
//     }
// }

//=============================================================================
// UI/ログへの送信用の型
//=============================================================================

/// 通知UIへの送信機能
/// 各関数の戻り値型に実装する
pub trait Notify {

    /// UIへのデータ送信用の型に変換
    fn to_dto(&self) -> NotifyDTO;


    /// 自身のデータをUIへ送信
    fn send(&self, app_handle: &tauri::AppHandle, is_desktop_notify: bool) {
        use tauri::Emitter;
        use tauri_plugin_notification::NotificationExt;

        let dto = self.to_dto();

        // デバッグ時のみコンソールへ表示
        if cfg!(debug_assertions) {
            println!("{}: {}", dto.title, dto.body);
        }

        // Debug → コンソール以外には表示しない
        if dto.level == NotifyLevel::Debug { return; }

        // TauriのイベントシステムでJSへ送信
        // 第一引数: イベント名(自由)
        // 第二引数: 送信するデータ
        app_handle.emit("log-event", &dto).eprint("JSへのログ通知に失敗");

        // デスクトップ通知を行うか判定
        if  !is_desktop_notify ||
            dto.level == NotifyLevel::Silent ||
            dto.level == NotifyLevel::ErrorSilent {
            return;
        }

        // move用コピー
        let app_handle_clone = app_handle.clone();

        // メインスレッドから通知しないと、Windowsは1度保留にしてしまう
        app_handle.run_on_main_thread(move || {

            // デスクトップ通知を送信
            // ※ インストーラを使用しない場合、アプリ名がPowerShellになる
            app_handle_clone.notification()
                .builder()
                .title(dto.title)
                .body(dto.body)
                .show()
                .eprint("デスクトップ通知に失敗");

        }).eprint("メインスレッドへの委託に失敗");
    }
}


/// UIへのデータ送信用の型
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyDTO {
    pub level: NotifyLevel,  // ユーザーへの通知レベル
    pub title: &'static str, // デスクトップ通知のタイトル部分 (発生イベントの説明)
    pub body: String,        // デスクトップ通知のボディ部分 (ファイル名など)
}


/// ユーザーへの通知レベル
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")] // JSでは「info, error, silent」になる
pub enum NotifyLevel {
    ErrorSilent,  // 赤文字: GUI表示のみ (デスクトップ通知なし / 個別にダイアログ表示などを行う)
    Error,        // 赤文字: GUI表示 + デスクトップ通知
    Info,         // 緑文字: GUI表示 + デスクトップ通知
    Silent,       // 灰文字: GUI表示のみ (デスクトップ通知なし)
    Debug,        // コンソール表示のみ (開発用)
}

//=============================================================================
// timer.rs の通知型
//=============================================================================

pub enum TimerInfo {
    Elapsed { minutes: u64 },   // 未保存時間が指定分経過した
    // Reset,                   // バックアップされ、未保存時間をリセットした
    // Start,
    // Stop,                    // 計測スレッドが終了
}

impl Notify for TimerInfo {
    fn to_dto(&self) -> NotifyDTO {
        use NotifyLevel::*;
        use TimerInfo::*;

        let (level, title, body) = match self {
            Elapsed{minutes} => (Info, "ファイル未保存期間", format!("{minutes}分経過")),
        };

        NotifyDTO { level, title, body }
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

impl Notify for ConfigError {
    fn to_dto(&self) -> NotifyDTO {
        use NotifyLevel::*;
        use ConfigError::*;

        let (level, title, body) = match self {
            InvalidSourcePath      => (ErrorSilent, "開始失敗", "バックアップ元フォルダが無効です".into()),
            InvalidDestinationPath => (ErrorSilent, "開始失敗", "バックアップ先フォルダが無効です".into()),
            PathConflict           => (ErrorSilent, "開始失敗", "バックアップ元とバックアップ先が同じです".into()),
            NoExtension            => (ErrorSilent, "開始失敗", "拡張子を一つ以上設定してください".into()),
            InvalidNotifyInterval  => (ErrorSilent, "開始失敗", "未保存通知は1分以上に設定してください".into()),
        };

        NotifyDTO { level, title, body }
    }
}

//=============================================================================
// watch.rs のイベント情報型 / 戻り値型
//=============================================================================

/// フォルダ監視中のイベント情報 (UI/ログへの送信用)
#[derive(Debug, Clone, serde::Serialize)]
pub enum WatchInfo {
    ModificationDetected(PathBuf), // ファイル変更の検出
    NotTarget(PathBuf),            // バックアップの対象外のためスキップ
    DebounceError,                 // DebounceEventResult のエラー
}

impl Notify for WatchInfo {
    fn to_dto(&self) -> NotifyDTO {
        use NotifyLevel::*;
        use WatchInfo::*;

        let (level, title, body) = match self {
            ModificationDetected(path) => (Debug, "変更を検出", get_filename(path)),
            NotTarget(path)            => (Debug, "対象外のためスキップ", get_filename(path)),
            DebounceError              => (Error, "フォルダ監視中のエラー", "デバウンスエラーが発生".into()),
        };

        NotifyDTO { level, title, body }
    }
}


/// フォルダ監視の開始処理の結果 (UI/ログへの送信用)
#[derive(Debug, Clone, serde::Serialize)]
pub enum StartResult {
    Success,                    // フォルダ監視の開始に成功
    AlreadyRunning,             // 既に監視が開始している
    NewDebouncerFailed,         // デバウンサーの生成に失敗
    DebounceStartFailed,        // デバウンサーが監視開始に失敗
}

impl Notify for StartResult {
    fn to_dto(&self) -> NotifyDTO {
        use NotifyLevel::*;
        use StartResult::*;

        let (level, title, body) = match self {
            Success                => (Info,  "バックアップを開始しました", "".into()),
            AlreadyRunning         => (Info,  "既にフォルダ監視中", "".into()),
            NewDebouncerFailed     => (Error, "開始失敗", "デバウンサーの生成に失敗しました".into()),
            DebounceStartFailed    => (Error, "開始失敗", "デバウンサーの開始に失敗しました".into()),
        };

        NotifyDTO { level, title, body }
    }
}

/// フォルダ監視の停止処理の結果 (UI/ログへの送信用)
#[derive(Debug, Clone, serde::Serialize)]
pub enum StopResult {
    Success,        // フォルダ監視を終了
    AlreadyStopped, // 既に停止中
}

impl Notify for StopResult {
    fn to_dto(&self) -> NotifyDTO {
        use NotifyLevel::*;
        use StopResult::*;

        let (level, title, body) = match self {
            Success        => (Info, "バックアップを停止しました", "".into()),
            AlreadyStopped => (Info, "既に停止済み", "".into()),
        };

        NotifyDTO { level, title, body }
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

impl Notify for WaitResult {
    fn to_dto(&self) -> NotifyDTO {
        use NotifyLevel::*;
        use WaitResult::*;

        let (level, title, body) = match self {
            Success       => (Debug, "ファイルの書込み終了を検知", "".into()),
            Locked(path)  => (Error, "ファイルがロック中", get_filename(path)),
            Missing(path) => (Error, "ファイル消失または読取権限なし", get_filename(path)),
        };

        NotifyDTO { level, title, body }
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

impl Notify for BackupResult {
    fn to_dto(&self) -> NotifyDTO {
        use BackupResult::*;
        use NotifyLevel::*;

        let (level, title, body) = match self {
            Copied         (path) => (Info, "バックアップ", get_filename(path)),
            AlreadyExists  (path) => (Silent, "既にバックアップ済み", get_filename(path)),
            InvalidFileName(path) => (Error, "ファイル名の取得に失敗", get_filename(path)),
            MetadataFailed (path) => (Error, "ファイル情報の取得に失敗", get_filename(path)),
            ModifiedFailed (path) => (Error, "最終更新時の取得に失敗", get_filename(path)),
            CopyFailed     (path) => (Error, "コピー失敗", get_filename(path)),
        };

        NotifyDTO { level, title, body }
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
