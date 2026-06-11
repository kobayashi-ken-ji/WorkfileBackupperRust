use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use futures::FutureExt;
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer, notify::*};
use tokio::sync::mpsc::Sender;  // Tokio版を使用すること

use crate::models::config::Config;
use crate::models::notify::{Notifier, StartResult, StopResult, WaitResult, WatchInfo};
use crate::services::file_manager::ActiveFileManager;
use crate::services::target_checker::TargetChecker;
use crate::services::wait::wait_for_file_writing;
use crate::services::backup;
use crate::services::timer;
use crate::utilities::{ResutlErrPrint, lock_mutex};



/// デバウンサ処理と処理中ファイルリストを統合
pub struct Watcher {
    file_manager: Arc<ActiveFileManager>,

    // TauriのStateへの登録は固定のまま、デバウンサだけ使い捨てにする
    // Optionが Ok→稼働中、None→停止中
    debouncer: Mutex<Option<Debouncer<RecommendedWatcher, RecommendedCache>>>,
}

impl Watcher {

    /// コンストラクタ
    pub fn new(tokio_handle: &tokio::runtime::Handle) -> Self {
        Self {
            file_manager: Arc::new(ActiveFileManager::new(tokio_handle.clone())),
            debouncer: Mutex::new(None),
        }
    }


    /// 「開始」ボタンが押されたときの処理
    /// 
    /// config は validate() 済みであることが必要
    /// 戻り値： フォルダ監視の開始に成功したか
    pub fn start(&self, config: Config, notifier: impl Notifier,
        unsaved_notifier: impl Notifier) -> std::result::Result<(), ()> {

        use StartResult::*;

        // move用にクローン
        let file_manager = self.file_manager.clone();
        let notifier_clone = notifier.clone();

        // configからクローン (config本体はmoveする)
        let source_path = config.source_path.clone();
        let recursive = config.recursive;

        // 「バックアップ対象か」の判定機を生成
        let target_checker = TargetChecker::new(
            config.all_files_enabled,
            &config.extensions
        );

        // 「ファイル未保存時間」の計測用スレッド作成
        // 計測しない場合は None
        let timer_tx = if config.is_notify_unsaved {
            let tx = timer::run_timer(config.notify_interval, unsaved_notifier);
            Some(tx)
        } else {
            None
        };

        //---------------------------------------------------------------
        
        // 処理中ファイルのリストを排他ロックする
        let mut lock_debouncer = lock_mutex(&self.debouncer);

        // 既に開始していたら終了 (二重起動防止)
        if lock_debouncer.is_some() {
            notifier.notify(&AlreadyRunning);
            return Err(());
        }

        // デバウンサを生成
        // フォルダ内を監視し、変更発生時に引数関数を実行する
        // 指定時間以上イベント通知が途切れるのを待つ
        let debouncer = new_debouncer(
            Duration::from_secs(5), None, move |result: DebounceEventResult| {

            // moveした値を参照で渡す
            Self::debouncer_callback(
                result, &file_manager, &config,
                &target_checker, &timer_tx, &notifier_clone
            );
        });

        // デバウンサ生成のエラー処理
        let mut debouncer = match debouncer {
            Ok(debouncer) => debouncer,
            Err(error) => {
                eprintln!("{error}");
                notifier.notify(&NewDebouncerFailed);
                return Err(());
            }
        };

        // 「サブフォルダを含まない」の反映
        let mode = if recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };

        // フォルダを監視対象に登録
        if let Err(error) = debouncer.watch(source_path, mode) {
            eprintln!("{error}");
            notifier.notify(&DebounceStartFailed);
            return Err(());
        };

        // デバウンサをフィールドに保持
        *lock_debouncer = Some(debouncer);

        // フォルダ監視開始の成功を通知
        notifier.notify(&Success);
        Ok(())
    }


    /// デバウンサーへ渡す関数
    /// フォルダ内の変更を検出後の処理
    fn debouncer_callback(
        result: DebounceEventResult, file_manager: &Arc<ActiveFileManager>,
        config: &Config, target_checker: &TargetChecker,
        timer_tx: &Option<Sender<()>>, notifier: &impl Notifier
    ) {
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

                        // バックアップ対象かを判定
                        if !target_checker.is_target(&path) {
                            notifier.notify( &WatchInfo::NotTarget(path) );
                            continue;
                        }

                        // 変更検出を通知
                        notifier.notify( &WatchInfo::Detected(path.clone()) );

                        // ファイル変更終了待ち + バックアップ処理
                        Self::wait_and_backup(
                            file_manager,
                            config.source_path.clone(),
                            config.destination_path.clone(),
                            &path,
                            timer_tx.clone(),
                            notifier.clone(),
                        );
                    }
                }
            }

            Err(errors) => {
                eprintln!("{:#?}", errors);
                notifier.notify(&WatchInfo::DebounceError);
            }
        }
    }


    /// ファイル変更終了待ち + バックアップ処理
    fn wait_and_backup(
        file_manager: &Arc<ActiveFileManager>, 
        source_path: PathBuf,
        destination_path: PathBuf,
        path: &PathBuf,
        timer_tx: Option<Sender<()>>,
        notifier: impl Notifier,
    ) {
        // 「処理中ファイルリスト」に登録する
        file_manager.execute(&path, move |path| {

            // スレッドに渡すため、BoxFuture化
            async move {

                // 書込みが終了するまで待機
                let result = wait_for_file_writing(&path).await;
                notifier.notify(&result);

                // 待機失敗時はスキップ
                if !matches!(result, WaitResult::Success) {
                    return;
                }

                // バックアップを実行
                let result = backup::backup_file(&source_path, &destination_path, &path);
                notifier.notify(&result);
                
                // 未保存時間をリセット
                // None → 通知OFFの状態
                if let Some(timer_tx) = timer_tx {
                    timer_tx.send(()).await.eprint("未保存時間リセット信号の送信エラー");
                }

            }.boxed()
        });
    }


    /// 「停止」ボタンが押されたときの処理
    /// (既に停止中でも実行可)
    pub async fn stop(&self) -> StopResult {

        // std版Mutexは、ロック状態では await を使用できないため、
        // ブロックでドロップさせている
        {
            // ロック解除 + ポイズンエラー処理
            let mut lock_debouncer = lock_mutex(&self.debouncer);

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
