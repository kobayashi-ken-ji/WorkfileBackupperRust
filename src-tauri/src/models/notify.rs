use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use crate::models::eprint::ResutlErrPrint;

//=============================================================================
// UI/ログへの送信用の型
//=============================================================================

// 各関数の戻り値に実装するトレイト
pub trait Notify {
    const SEND_ERROR: &str = "メッセージ受信機がドロップされています";

    /// UI/ログへのデータ型に変換
    fn to_dto(&self) -> NotifyDTO;

    /// 自身のデータを送信
    fn send(&self, tx: &Sender<NotifyPackage>) {
        let package = NotifyPackage::Message(self.to_dto());
        tx.send(package).eprint(Self::SEND_ERROR);
    }
}


/// UI/ログへの送信機に渡す型
pub enum NotifyPackage {
    Message(NotifyDTO),         // 通常の通知データ
    Config { is_notify: bool }, // デスクトップ通知のON/OFF切替えを指示
}


/// UI/ログへの送信用の型
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
    Error,  // 赤文字: GUI表示 + デスクトップ通知
    Info,   // 緑文字: GUI表示 + デスクトップ通知
    Silent, // 灰文字: GUI表示のみ (デスクトップ通知なし)
    Debug,  // コンソール表示のみ (開発用)
            // Warn,
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
// watch.rs のイベント情報型 / 戻り値型
//=============================================================================

/// フォルダ監視中のイベント情報 (UI/ログへの送信用)
#[derive(Debug, Clone, serde::Serialize)]
pub enum WatchInfo {
    ModificationDetected(PathBuf), // ファイル変更の検出
    UnspecifiedExtension(PathBuf), // 指定外の拡張子のためスキップ
    DebounceError,                 // DebounceEventResult のエラー
}

impl Notify for WatchInfo {
    fn to_dto(&self) -> NotifyDTO {
        use NotifyLevel::*;
        use WatchInfo::*;

        let (level, title, body) = match self {
            ModificationDetected(path) => (Debug, "変更を検出", get_filename(path)),
            UnspecifiedExtension(path) => (Debug, "指定外の拡張子をスキップ", get_filename(path)),
            DebounceError              => (Error, "フォルダ監視中のエラー", "デバウンスエラーが発生".into()),
        };

        NotifyDTO { level, title, body }
    }
}

/// フォルダ監視の開始処理の結果 (UI/ログへの送信用)
#[derive(Debug, Clone, serde::Serialize)]
pub enum StartResult {
    Success,                    // フォルダ監視の開始に成功
    InvalidSourcePath,          // バックアップ元フォルダが無効
    InvalidDestinationPath,     // バックアップ先フォルダが無効
    PathConflict,               // バックアップ元/先 が同じ
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
            InvalidSourcePath      => (Error, "開始失敗", "バックアップ元フォルダが無効です".into()),
            InvalidDestinationPath => (Error, "開始失敗", "バックアップ先フォルダが無効です".into()),
            PathConflict           => (Error, "開始失敗", "バックアップ元とバックアップ先が同じです".into()),
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
            Success       => (Debug, "ファイルの書込み終了を検知", "".into()), // UIには送信されない想定
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
