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
    BackupResult, Notify, NotifyDTO, StartResult, StopResult, WaitResult, WatchInfo
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
    
    /// 別スレッドがパニックするとロックが汚染される。中の値を強制取得して続行。
    const MUTEX_POISON_ERR: &str
        = "ミューテックスのポイズンエラーが発生。強制取得して続行します。";


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
    pub fn start(&self, config: &Config, tx: Sender<NotifyDTO>) -> StartResult {

        use StartResult::*;
        const SEND_ERROR: &'static str = "メッセージ受信機がドロップされています";

        // バックアップ対象にする拡張子のリスト
        let extensions = Extensions::from(config.extensions.as_slice());

        // 監視先フォルダ
        // canonicalize: 正規化 (絶対パス化 + 余計な/や.を削除)
        let watch_path = match config.source_path.canonicalize() {
            Ok(path) => path,
            Err(error) => {
                eprintln!("{error}");
                return InvalidSourcePath(config.source_path.clone());
            }
        };
        
        // バックアップ先フォルダを指定
        let destination_path = match config.destination_path.canonicalize() {
            Ok(path) => path,
            Err(error) => {
                eprintln!("{error}");
                return InvalidDestinationPath(config.destination_path.clone());
            }
        };

        // バックアップ用インスタンスを生成
        let backupper = FileBackupper::new(&destination_path);
        if !backupper.is_valid() {
            // バックアップ先がフォルダではない
            return InvalidDestinationPath(destination_path);
        }

        //---------------------------------------------------------------
        
        // // [!] unwrap処理を忘れずに
        // let mut lock_debouncer = self.debouncer.lock().unwrap();

        // 処理中ファイルのリストを排他ロックする
        let mut lock_debouncer = match self.debouncer.lock() {
            Ok(guard) => guard,
            Err(poison_err) => {
                println!("{}", Self::MUTEX_POISON_ERR);
                poison_err.into_inner()
            }
        };

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
                                tx.send(info.get_dto()).expect(SEND_ERROR);
                                continue;
                            }

                            // 変更検出を通知
                            let info = WatchInfo::ModificationDetected(path.clone());
                            tx.send(info.get_dto()).expect(SEND_ERROR);
                            
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
                                            tx.send(result.get_dto()).expect(SEND_ERROR);
                                        }

                                        // 待機失敗時: メッセージを送信
                                        _ => {tx.send(result.get_dto()).expect(SEND_ERROR)}
                                    }
                                }.boxed()
                            });
                        }
                    }
                }
                Err(errors) => {
                    eprintln!("{:#?}", errors);
                    let info = WatchInfo::DebounceError;
                    tx.send(info.get_dto()).expect(SEND_ERROR);
                }
            }
        });

        let mut debouncer = match debouncer {
            Ok(debouncer) => debouncer,
            Err(error) => {
                eprintln!("{error}");
                return NewDebouncerFailed;
            }
        };

        // フォルダを監視対象に登録 (NonRecursive = サブフォルダを含まない)
        if let Err(error) = debouncer.watch(watch_path.clone(), RecursiveMode::NonRecursive) {
            eprintln!("{error}");
            return DebounceStartFailed(watch_path);
        };

        // デバウンサを自身に保持
        *lock_debouncer = Some(debouncer);

        Success
    }


    // 「停止」ボタンが押されたときの処理 (開始していなくてもエラーなし)
    pub async fn stop(&self) -> StopResult {

        // std版Mutexは、ロックがあると await を使用できないため、
        // ブロックでドロップさせている
        {
            let mut lock_debouncer = match self.debouncer.lock() {
                Ok(guard) => guard,
                Err(poison_err) => {
                    println!("{}", Self::MUTEX_POISON_ERR);
                    poison_err.into_inner()
                }
            };

            // 既に停止中かチェック
            if lock_debouncer.is_none() {
                return StopResult::AlreadyStopped;
            }

            // takeでデバウンサーを抜き出す / 代わりにNoneが入る
            if let Some(debouncer) = lock_debouncer.take() {

                // フォルダ監視を解除
                let _ = debouncer.stop_nonblocking();

                // スコープを抜ける時、
                // debouncerがドロップ(引数のクロージャも含む) → 使い捨て完了
            }
        }

        // 実行中のタスクの終了を待機
        self.file_manager.join_tasks().await;
        StopResult::Success
    }
}