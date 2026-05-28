use std::path::{Path, PathBuf};

//=============================================================================
// 送信用型へのトレイト
//=============================================================================

/// UI/ログへの送信用の型 の共通処理
pub trait AppMessage {

    /// UI表示用メッセージを取得
    fn to_ui_message(&self) -> String;

    /// 保持している値がエラーかを判定
    fn is_error(&self) -> bool;
}

//=============================================================================
// 送信用の型を集約
//=============================================================================

/// UI/ログへの送信用の型 (情報型を1つに集約)
#[derive(Debug, Clone, serde::Serialize)]
// #[serde(tag = "type", content = "payload")] // JS側で扱いやすくする工夫
pub enum NotifyMessage {
    Start(StartResult),
    Watch(WatchInfo),
    Wait(WaitResult),
    Backup(BackupResult),
}

// use models::AppMessage; を省略できるように、トレイトを使用しない
impl NotifyMessage {

    pub fn to_ui_message(&self) -> String {
        use NotifyMessage::*;
        match self {
            Start(x)  => x.to_ui_message(),
            Watch(x)  => x.to_ui_message(),
            Wait(x)   => x.to_ui_message(),
            Backup(x) => x.to_ui_message(),
        }
    }

    pub fn is_error(&self) -> bool {
        use NotifyMessage::*;
        match self {
            Start(x)  => x.is_error(),
            Watch(x)  => x.is_error(),
            Wait(x)   => x.is_error(),
            Backup(x) => x.is_error(),
        }
    }
}

//=============================================================================
// watch.rs のイベント情報型 / 戻り値型
//=============================================================================

/// フォルダ監視中のイベント情報 (UI/ログへの送信用)
#[derive(Debug, Clone, serde::Serialize)]
pub enum WatchInfo {
    ModificationDetected(PathBuf),  // ファイル変更の検出
    UnspecifiedExtension(PathBuf),  // 指定外の拡張子のためスキップ
    DebounceError(String),          // DebounceEventResult のエラー (Vec<Error>を文字列化)
}

impl AppMessage for WatchInfo {

    fn to_ui_message(&self) -> String {
        use WatchInfo::*;
        match self {
            ModificationDetected(path) => format!("変更を検出: {}", clean_path(path)),
            UnspecifiedExtension(path) => format!("指定外の拡張子をスキップ: {}", clean_path(path)),
            DebounceError(errors)      => format!("デバウンスエラー: {:?}", errors),
        }
    }

    fn is_error(&self) -> bool {
        use WatchInfo::*;
        match self {
            ModificationDetected(_) => false,
            UnspecifiedExtension(_) |
            DebounceError(_) => true,
        }
    }
}


/// フォルダ監視の開始処理の結果 (UI/ログへの送信用)
#[derive(Debug, Clone, serde::Serialize)]
pub enum StartResult {
    Success,                                                 // フォルダ監視の開始に成功
    InvalidSourcePath { path: PathBuf, error: String },      // バックアップ元フォルダが無効
    InvalidDestinationPath { path: PathBuf, error: String }, // バックアップ先フォルダが無効
    AlreadyRunning,                                          // 既に監視が開始している
    NewDebouncerFailed(String),                              // デバウンサーの生成に失敗
    DebounceStartFailed { path: PathBuf, error: String },    // デバウンサーが監視開始に失敗
}

impl AppMessage for StartResult {

    fn to_ui_message(&self) -> String {
        use StartResult::*;
        match self {
            Success                       => format!("バックアップを開始しました"),
            InvalidSourcePath{path, ..}   => format!("バックアップ元フォルダが無効: {}", clean_path(path)),
            InvalidDestinationPath{path, ..} => format!("バックアップ先フォルダが無効: {}", clean_path(path)),
            AlreadyRunning                => format!("既にフォルダ監視中"),
            NewDebouncerFailed(error)     => format!("デバウンサーの生成に失敗: {:?}", error),
            DebounceStartFailed{path, ..} => format!("デバウンサーが監視開始に失敗: {}", clean_path(path)),
        }
    }

    fn is_error(&self) -> bool {
        use StartResult::*;
        match self {
            Success => false,
            InvalidSourcePath {..} |
            InvalidDestinationPath {..} |
            AlreadyRunning |
            NewDebouncerFailed(_) |
            DebounceStartFailed {..}  => true,
        }
    }
}

//=============================================================================
// wait.rs の戻り値型
//=============================================================================

/// ファイル書込終了待ちの結果 (UI/ログへの送信用)
#[derive(Debug, Clone, serde::Serialize)]
pub enum WaitResult {
    Success,            // 書込終了を確認した
    Locked(PathBuf),    // ファイルがロックされている
    Missing(PathBuf),   // ファイルを見失った
}

impl AppMessage for WaitResult {

    fn to_ui_message(&self) -> String {
        use WaitResult::*;
        match self {
            Success        => format!("ファイルの書込みが終了"),  // UIには送信されない想定
            Locked  (path) => format!("ファイルがロック中: {}", clean_path(path)),
            Missing (path) => format!("ファイル消失または読取権限なし: {}", clean_path(path)),
        }
    }

    fn is_error(&self) -> bool {
        use WaitResult::*;
        match self {
            Success => false,
            Locked(_) |
            Missing(_) => true,
        }
    }
}

//=============================================================================
// backup.rs の戻り値型
//=============================================================================

// バックアップ処理の結果 (UI/ログへの送信用)
#[derive(Debug, Clone, serde::Serialize)]
pub enum BackupResult {
    Copied          (PathBuf),
    AlreadyExists   (PathBuf),
    InvalidFileName (PathBuf),
    MetadataFailed  { path: PathBuf, error: String },  // std::io::Errorを文字列化
    ModifiedFailed  { path: PathBuf, error: String },
    CopyFailed      { path: PathBuf, error: String },
}

impl AppMessage for BackupResult {

    fn to_ui_message(&self) -> String {
        use BackupResult::*;
        match self {
            Copied          (path)     => format!("バックアップ: {}", clean_path(path)),
            AlreadyExists   (path)     => format!("既にバックアップ済み: {}", clean_path(path)),
            InvalidFileName (path)     => format!("ファイル名の取得に失敗: {}", clean_path(path)),
            MetadataFailed  {path, ..} => format!("ファイル情報の取得に失敗: {}", clean_path(path)),
            ModifiedFailed  {path, ..} => format!("最終更新時の取得に失敗: {}", clean_path(path)),
            CopyFailed      {path, ..} => format!("コピーに失敗: {}", clean_path(path)),
        }
    }

    fn is_error(&self) -> bool {
        use BackupResult::*;
        match self {
            Copied(_) |
            AlreadyExists(_) => false,

            InvalidFileName(_) |
            MetadataFailed{..} |
            ModifiedFailed{..} |
            CopyFailed{..} => true,
        }
    }
}

//=============================================================================
// ユーティリティ
//=============================================================================

/// 拡張接頭辞を外す (UI表示用)
/// PathBuf::canonicalize() すると「\\?\」が付与されるため、表示用に削除する
fn clean_path(path: &Path) -> String {

    // PathBuf → String 変換
    // ※UTF-8として不正な部分は「」に変換する
    let path_str = path.to_string_lossy();

    // Windows環境のみ拡張接頭辞を外す
    #[cfg(windows)]
    if path_str.starts_with(r"\\?\") {
        return path_str[4..].to_string()
    }

    path_str.into_owned()
}


// fn test_path_buf(path: String) -> Result<(), String> {

//     // String → PathBuf 変換
//     let path_buf = PathBuf::from(&path);
//     println!("有効なパスか: {}", path_buf.exists());

//     // PathBuf → String 変換
//     // ※UTF-8として不正な部分は「」に変換する
//     let _path = path_buf.to_string_lossy().into_owned();

//     // 正規化 (絶対パス化 + 余計な/や.を削除)
//     // Windowsでは、UNC/拡張接頭辞(\\?\) を付与 (260文字制限を解決)
//     let canonical_path = path_buf.canonicalize()
//         .map_err(|e| format!("パスが正しくない、または存在しない: {e}"))?;

//     Ok(())
// }