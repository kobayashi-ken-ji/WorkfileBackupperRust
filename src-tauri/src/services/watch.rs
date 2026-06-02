use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Sender};
use std::time::Duration;
use futures::FutureExt;
use notify_debouncer_full::{notify::*, DebounceEventResult, new_debouncer, Debouncer, RecommendedCache};
use notify_debouncer_full::notify::{RecommendedWatcher};
use std::convert::From;

use crate::models::config::Config;
use crate::models::message::{
    Notify, NotifyPackage, StartResult, StopResult, WaitResult, WatchInfo
};

use crate::services::file_manager::{ActiveFileManager};
use crate::services::wait::wait_for_file_writing;
use crate::services::backup::{FileBackupper};
use crate::services::extensions::Extensions;


/// デバウンサ処理と処理中ファイルリストを統合
pub struct Watcher {
    file_manager: Arc<ActiveFileManager>,

    // TauriのStateへの登録は固定のまま、デバウンサだけ使い捨てにする
    // Optionが Ok→稼働中、None→停止中
    debouncer: Mutex<Option<Debouncer<RecommendedWatcher, RecommendedCache>>>,
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
        }
    }


    /// 「開始」ボタンが押されたときの処理
    /// 戻り値： フォルダ監視の開始に成功したか
    pub fn start(&self, config: &Config, tx: Sender<NotifyPackage>) -> std::result::Result<(), ()> {

        use StartResult::*;

        // バックアップ対象にする拡張子のリスト
        let extensions = Extensions::from(config.extensions.as_slice());

        // 監視先フォルダ
        // canonicalize: 正規化 (絶対パス化 + 余計な/や.を削除)
        let watch_path = match config.source_path.canonicalize() {
            Ok(path) => path,
            Err(error) => {
                eprintln!("{error}");
                InvalidSourcePath.send(&tx);
                return Err(());
            }
        };
        
        // バックアップ先フォルダを指定
        let destination_path = match config.destination_path.canonicalize() {
            Ok(path) => path,
            Err(error) => {
                eprintln!("{error}");
                InvalidDestinationPath.send(&tx);
                return Err(());
            }
        };

        // バックアップ元とバックアップ先が同じ
        if watch_path == destination_path {
            PathConflict.send(&tx);
            return Err(());
        }
        
        // バックアップ用インスタンスを生成
        let backupper = FileBackupper::new(&destination_path);
        if !backupper.is_valid() {
            // バックアップ先がフォルダではない
            InvalidDestinationPath.send(&tx);
            return Err(());
        }

        //---------------------------------------------------------------
        
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
            AlreadyRunning.send(&tx);
            return Err(());
        }

        // クローン用の中間変数
        let file_manager_clone = self.file_manager.clone();

        // デバウンサ (2秒間イベントが途切れるのを待つ)
        // 毎回新しく生成
        let tx_clone = tx.clone();
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
                                WatchInfo::UnspecifiedExtension(path).send(&tx);
                                continue;
                            }

                            // 変更検出を通知
                            WatchInfo::ModificationDetected(path.clone()).send(&tx);

                            // move用にコピー
                            let tx = tx.clone();
                            let backupper = backupper.clone();

                            // ファイル変更終了待ち + バックアップ処理
                            file_manager_clone.execute(&path, |path| {
                                async move {
                                    // 書込みが終了するまで待機
                                    let result = wait_for_file_writing(&path).await;
                                    match result {

                                        // 待機成功時: ファイルをバックアップ
                                        WaitResult::Success => {
                                            backupper.backup_file(&path).send(&tx);
                                        }

                                        // 待機失敗時: メッセージを送信
                                        _ => result.send(&tx),
                                    }
                                }.boxed()
                            });
                        }
                    }
                }
                Err(errors) => {
                    eprintln!("{:#?}", errors);
                    WatchInfo::DebounceError.send(&tx);
                }
            }
        });

        let mut debouncer = match debouncer {
            Ok(debouncer) => debouncer,
            Err(error) => {
                eprintln!("{error}");
                NewDebouncerFailed.send(&tx_clone);
                return Err(());
            }
        };

        // フォルダを監視対象に登録 (NonRecursive = サブフォルダを含まない)
        if let Err(error) = debouncer.watch(watch_path, RecursiveMode::NonRecursive) {
            eprintln!("{error}");
            DebounceStartFailed.send(&tx_clone);
            return Err(());
        };

        // デバウンサをフィールドに保持
        *lock_debouncer = Some(debouncer);

        // フォルダ監視開始の成功を通知
        Success.send(&tx_clone);
        Ok(())
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
                debouncer.stop_nonblocking();

                // スコープを抜ける時、
                // debouncerがドロップ(引数のクロージャも含む) → 使い捨て完了
            }
        }

        // 実行中のタスクの終了を待機
        self.file_manager.join_tasks().await;
        StopResult::Success
    }
}