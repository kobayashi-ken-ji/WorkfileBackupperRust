use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Sender, Receiver, channel};
use std::time::Duration;
use futures::channel::mpsc;
use tokio::runtime::Runtime;
use futures::FutureExt;
use notify_debouncer_full::{notify::*, DebounceEventResult, new_debouncer, Debouncer, RecommendedCache};
use notify_debouncer_full::notify::{self, RecommendedWatcher};
use std::convert::From;

use crate::models::config::Config;
use crate::models::message::{
    NotifyMessage, WatchInfo, StartResult, WaitResult, BackupResult
};

use crate::services::file_manager::{self, ActiveFileManager};
use crate::services::file_wait::{wait_file_writing};
use crate::services::backup::{FileBackupper};
use crate::services::extensions::Extensions;


/// デバウンサ処理と処理中ファイルリストを統合
pub struct Watcher {
    file_manager: Arc<ActiveFileManager>,

    // TauriのStateへの登録は固定のまま、デバウンサだけ使い捨てにする
    // Optionが Ok→稼働中、None→停止中
    debouncer: Mutex<Option<Debouncer<RecommendedWatcher, RecommendedCache>>>,

    // Tokioランタイムのハンドル
    tokio_handle: tokio::runtime::Handle,
}

impl Watcher {

    /// コンストラクタ
    pub fn new(tokio_handle: &tokio::runtime::Handle) -> Self {
        Self {
            file_manager: Arc::new(ActiveFileManager::new(tokio_handle.clone())),
            debouncer: Mutex::new(None),
            tokio_handle: tokio_handle.clone(),
        }
    }


    /// 「開始」ボタンが押されたときの処理
    /// 戻り値： フォルダ監視の開始に成功したか
    pub fn start(&self, config: &Config, tx: Sender<NotifyMessage>) -> StartResult {

        use StartResult::*;
        const SEND_ERROR: &'static str = "メッセージ受信機がドロップされています";

        // バックアップ対象にする拡張子のリスト
        let extensions = Extensions::from(config.extensions.as_slice());

        // 監視先フォルダ
        // canonicalize: 正規化 (絶対パス化 + 余計な/や.を削除)
        let watch_path = match config.source_path.canonicalize() {
            Ok(path) => path,
            Err(error) => return InvalidSourcePath {
                path: config.source_path.clone(),
                error: error.to_string()
            },
        };

        // バックアップ先フォルダを指定
        let destination_path = match config.destination_path.canonicalize() {
            Ok(path) => path,
            Err(error) => return InvalidDestinationPath {
                path: config.destination_path.clone(),
                error: error.to_string()
            },
        };

        // バックアップ用インスタンスを生成
        let backupper = FileBackupper::new(&destination_path);
        if !backupper.is_valid() {
            return InvalidDestinationPath {
                path: destination_path,
                error: String::from("バックアップ先がフォルダではありません。"),
            };
        }

        //---------------------------------------------------------------
        let mut lock_debouncer = self.debouncer.lock().unwrap();

        // 既に開始していたら終了 (二重起動防止)
        if lock_debouncer.is_some() {
            return AlreadyRunning;
        }

        // クローン用の中間変数
        let file_manager_clone = self.file_manager.clone();

        // デバウンサ (2秒間イベントが途切れるのを待つ)
        // 毎回新しく生成
        let debouncer = new_debouncer(Duration::from_secs(2), None, move |result: DebounceEventResult| {
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
                            file_manager_clone.execute(&path, |path| {
                                async move {
                                    // 書込みが終了するまで待機
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
        });

        let mut debouncer = match debouncer {
            Ok(debouncer) => debouncer,
            Err(error) => return NewDebouncerFailed(error.to_string()),
        };

        // フォルダを監視対象に登録 (NonRecursive = サブフォルダを含まない)
        if let Err(error) = debouncer.watch(watch_path.clone(), RecursiveMode::NonRecursive) {
            return DebounceStartFailed {
                path: watch_path,
                error: error.to_string(),
            }
        };

        // デバウンサを自身に保持
        *lock_debouncer = Some(debouncer);

        Success
    }


    // 「停止」ボタンが押されたときの処理 (開始していなくてもエラーなし)
    pub async fn stop(&self, watch_path: &Path) {

        // std版Mutexは、ロックがあると await を使用できないため、
        // ブロックでドロップさせている
        {
            let mut lock = match self.debouncer.lock() {
                Ok(guard) => guard,
                Err(poison_err) => {
                    println!("ミューテックスのポイズンエラーが発生。強制取得して続行します。");
                    poison_err.into_inner()
                }
            };

            // takeでデバウンサーを抜き出す / 代わりにNoneが入る
            if let Some(mut debouncer) = lock.take() {

                // フォルダ監視を解除
                let _ = debouncer.unwatch(watch_path);

                // スコープを抜ける時、
                // debouncerがドロップ(引数のクロージャも含む) → 使い捨て完了
            }
        }

        // 実行中のタスクの終了を待機
        self.file_manager.join_tasks().await;
    }
}