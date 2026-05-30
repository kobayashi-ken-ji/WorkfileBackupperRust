use std::path::{Path, PathBuf};
use std::sync::mpsc::{Sender, Receiver, channel};
use std::time::Duration;
use futures::channel::mpsc;
use notify_debouncer_full::{notify::*, DebounceEventResult, new_debouncer};
use tokio::runtime::Runtime;
use futures::FutureExt;

use crate::services::file_manager::ActiveFileManager;
use crate::services::file_wait::{wait_file_writing, WaitResult};
use crate::services::backup::{BackupResult, FileBackupper};
use crate::services::extensions::Extensions;

/// UI/ログへの送信用の型
#[derive(Debug, Clone, serde::Serialize)]
// #[serde(tag = "type", content = "payload")] // JS側で扱いやすくする工夫
pub enum NotifyMessage {
    Watch(WatchInfo),
    Wait(WaitResult),
    Backup(BackupResult),
}

impl NotifyMessage {

    /// UI表示用メッセージを取得
    pub fn to_ui_message(&self) -> String {
        use NotifyMessage::*;
        match self {
            Watch(info)    => info.to_ui_message(),
            Wait(result)   => result.to_ui_message(),
            Backup(result) => result.to_ui_message(),
        }
    }
}

//=============================================================================
// 情報送信用の型
//=============================================================================

/// フォルダ内監視の情報 (UI/ログへの送信用)
#[derive(Debug, Clone, serde::Serialize)]
pub enum WatchInfo {
    ModificationDetected(PathBuf),  // ファイル変更の検出
    UnspecifiedExtension(PathBuf),  // 指定外の拡張子のためスキップ
    DebounceError(String),          // DebounceEventResult のエラー (Vec<Error>を文字列化)
}

impl WatchInfo {

    /// UI表示用メッセージを取得
    pub fn to_ui_message(&self) -> String {
        use WatchInfo::*;
        match self {
            ModificationDetected(path) => format!("変更を検出: {}", path.display()),
            UnspecifiedExtension(path) => format!("指定外の拡張子をスキップ: {}", path.display()),
            DebounceError(errors)      => format!("デバウンスエラー: {:?}", errors),
        }
    }
}

//=============================================================================
// フォルダ内監視
//=============================================================================

pub fn run(tx: Sender<NotifyDTO>) -> Result<()> {

    // バックアップ対象にする拡張子のリスト
    let mut extensions = Extensions::new();
    extensions.insert("psd");
    extensions.insert("sai2");
    extensions.insert("txt");
    extensions.insert("tmp");   // ファイル消失を再現


    // 監視先フォルダ
    let watch_path = Path::new(r"D:\一時作業ファイル");
    if !watch_path.exists() {
        panic!("次のディレクトリは存在しません: {:?}", watch_path);
    }
    println!("対象フォルダ: {:?}", watch_path);

    // バックアップ先フォルダを指定
    let backupper = FileBackupper::new(r"E:\old【一時作業】");
    if !backupper.is_valid() {
        panic!("バックアップ先フォルダが存在しません");
    }

    //---------------------------------------------------------------

    // Tokioランタイムを生成
    let runtime = Runtime::new().unwrap();
    
    // 同一ファイルに処理が重複するのを回避する
    let active_file_manager = ActiveFileManager::new(runtime.handle().clone());

    const SEND_ERROR: &'static str = "メッセージ受信機がドロップされています";

    // デバウンサ (2秒間イベントが途切れるのを待つ)
    let mut debouncer = new_debouncer(Duration::from_secs(2), None, move |result: DebounceEventResult| {
        match result {
            Ok(events) => {

                // フォルダ内で発生した全てのイベント
                for debounced_event in events {
                    let kind = debounced_event.event.kind;

                    // ファイルの「修正・変更・新規作成」に絞る
                    let is_modify = kind.is_modify() || kind.is_create();
                    if !is_modify { continue; }

                    // 検出したファイル全てに処理
                    for path in debounced_event.event.paths {

                        // 対象拡張子かをチェック
                        if !extensions.contains(&path) {
                            let info = WatchInfo::UnspecifiedExtension(path);
                            tx.send(NotifyMessage::Watch(info)).expect(SEND_ERROR);
                            continue;
                        }

                        // 変更検出を通知
                        let info = WatchInfo::ModificationDetected(path.clone());
                        tx.send(NotifyMessage::Watch(info)).expect(SEND_ERROR);
                        
                        // move用にコピー
                        let tx = tx.clone();
                        let backupper = backupper.clone();

                        // ファイル変更終了待ち + バックアップ処理
                        active_file_manager.execute(&path, |path| {
                            async move {
                                // 書込みが終了するまでループ
                                let result = wait_file_writing(&path).await;
                                match result {

                                    // 待機成功時: ファイルをバックアップ
                                    WaitResult::Success => {
                                        let result = backupper.backup_file(&path);
                                        tx.send(NotifyMessage::Backup(result)).expect(SEND_ERROR);
                                    }

                                    // 待機失敗時: メッセージを送信
                                    _ => {tx.send(NotifyMessage::Wait(result)).expect(SEND_ERROR)}
                                }
                            }.boxed()
                        });
                    }
                }
            }
            Err(errors) => {
                // Vec<Error> → String化
                let error_string = errors.iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<String>>()
                    .join("\n");

                let info = WatchInfo::DebounceError(error_string);
                tx.send(NotifyMessage::Watch(info)).expect(SEND_ERROR);
            }
        }
    })?;

    // フォルダを監視対象に登録 (NonRecursive = サブフォルダを含まない)
    debouncer.watch(watch_path, RecursiveMode::NonRecursive)?;
    std::thread::park();

    //---------------------------------------------------------------

    // 受信用ループ
    // ユーザーへの通知
    // for received in rx {
    //     match received {
    //         NotifyMessage::Watch(info) => {
    //             println!("{}", info.to_ui_message());
    //         }

    //         NotifyMessage::Wait(result) => {
    //             println!("{}", result.to_ui_message());
    //         }

    //         NotifyMessage::Backup(result) => {
    //             println!("{}", result.to_ui_message());
    //         }
    //     }
    // }

    Ok(())
}
