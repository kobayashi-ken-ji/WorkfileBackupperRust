use std::sync::{Arc, Mutex};
use std::time::Duration;
use futures::FutureExt;
use notify_debouncer_full::{notify::*, DebounceEventResult, new_debouncer, Debouncer, RecommendedCache};
use notify_debouncer_full::notify::{RecommendedWatcher};
use std::convert::From;

use crate::models::config::Config;
use crate::models::eprint::ResutlErrPrint;
use crate::models::notify::{
    ConfigError, Notify, StartResult, StopResult, WaitResult, WatchInfo
};

use crate::services::file_manager::{ActiveFileManager};
use crate::services::utilities::lock_mutex;
use crate::services::wait::wait_for_file_writing;
use crate::services::backup;
use crate::services::extensions::Extensions;
use crate::services::timer;



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


    /// 設定値が正しいかチェック
    /// start() の実行前に必ず行う
    /// 値をチェックして、修正可能な部分を直して返す
    pub fn validate_config(&self, mut config: Config) -> std::result::Result<Config, ConfigError> {
        use ConfigError::*;

        // 監視するフォルダ
        // canonicalize: 正規化 (絶対パス化 + 余計な/や.を削除)
        config.source_path = match config.source_path.canonicalize() {
            Ok(path) => {
                if path.is_dir() { path }
                else { return Err(InvalidSourcePath); }
            }
            Err(error) => {
                eprintln!("{error}");
                return Err(InvalidSourcePath);
            }
        };
        
        // バックアップ先フォルダ
        config.destination_path = match config.destination_path.canonicalize() {
            Ok(path) => {
                if path.is_dir() { path }
                else { return Err(InvalidDestinationPath); }
            }
            Err(error) => {
                eprintln!("{error}");
                return Err(InvalidDestinationPath);
            }
        };

        // バックアップ元とバックアップ先が同じ
        if config.source_path == config.destination_path {
            return Err(PathConflict);
        }

        // バックアップするファイル
        if !config.all_files_enabled &&     // 「全てのファイル」がfalse
            config.extensions.len() < 1     // 拡張子がひとつも登録されていない
        {
            return Err(NoExtension);
        }

        // 未保存の通知
        if config.is_notify_unsaved &&      // 通知が有効
            config.notify_interval < 1      // 1分以下
        {
            return Err(InvalidNotifyInterval);
        }

        Ok(config)
    }


    /// 「開始」ボタンが押されたときの処理
    /// 戻り値： フォルダ監視の開始に成功したか
    pub fn start(&self, app: tauri::AppHandle, config: Config) -> std::result::Result<(), ()> {
        use StartResult::*;

        // バックアップの対象 (全てのファイル or 指定された拡張子リスト)
        let all_files_enabled = config.all_files_enabled;
        let extensions = Extensions::from(config.extensions.as_slice());

        // configから取り出す (validate_configで値チェック済み)
        let watch_path = config.source_path.clone();
        let destination_path = config.destination_path.clone();
        let recursive = config.recursive;

        // ファイル未保存時間の計測用スレッド作成
        let timer_tx = timer::run_timer(
            config.is_notify_unsaved,
            config.notify_interval,
            app.clone()
        );

        //---------------------------------------------------------------

        let is_desktop_notify = config.is_notify;
        
        // 処理中ファイルのリストを排他ロックする
        let mut lock_debouncer = lock_mutex(&self.debouncer);

        // 既に開始していたら終了 (二重起動防止)
        if lock_debouncer.is_some() {
            AlreadyRunning.send(&app, is_desktop_notify);
            return Err(());
        }

        // move用にクローンする
        // Arcで包んであるためクローン可
        let file_manager_clone = self.file_manager.clone();
        let app_clone = app.clone();

        // デバウンサ (2秒間イベントが途切れるのを待つ)
        // 毎回新しく生成
        // let tx_clone = tx.clone();
        let watch_path_clone = watch_path.clone();
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

                            // 全ファイルがバックアップ対象の場合
                            // ファイルかどうかのみチェック
                            if all_files_enabled {
                                if !path.is_file() { continue; }
                            }

                            // 対象拡張子かをチェック
                            else if !extensions.contains(&path) {
                                WatchInfo::UnspecifiedExtension(path).send(&app, is_desktop_notify);
                                continue;
                            }

                            // 変更検出を通知
                            WatchInfo::ModificationDetected(path.clone()).send(&app, is_desktop_notify);

                            // move用にコピー
                            let timer_tx = timer_tx.clone();
                            let destination_path = destination_path.clone();
                            let watch_path = watch_path.clone();
                            let app = app.clone();

                            // ファイル変更終了待ち + バックアップ処理
                            file_manager_clone.execute(&path, move |path| {
                                async move {
                                    // 書込みが終了するまで待機
                                    let result = wait_for_file_writing(&path).await;
                                    match result {

                                        // 待機成功時: ファイルをバックアップ
                                        WaitResult::Success => {

                                            // 相対パス対応のコピー先を生成
                                            let destination_path = if recursive {
                                                backup::get_destination_for_recursive(&watch_path, &destination_path, &path)
                                            } else {
                                                destination_path
                                            };
                                            
                                            backup::backup_file(&destination_path, &path).send(&app, is_desktop_notify);

                                            // 未保存時間をリセット
                                            // None → 通知OFFの状態
                                            if let Some(timer_tx) = timer_tx {
                                                timer_tx.send(()).await
                                                    .eprint("未保存時間リセット信号の送信エラー");
                                            }
                                        }

                                        // 待機失敗時: メッセージを送信
                                        _ => result.send(&app, is_desktop_notify),
                                    }
                                }.boxed()
                            });
                        }
                    }
                }
                Err(errors) => {
                    eprintln!("{:#?}", errors);
                    WatchInfo::DebounceError.send(&app, is_desktop_notify);
                }
            }
        });

        let mut debouncer = match debouncer {
            Ok(debouncer) => debouncer,
            Err(error) => {
                eprintln!("{error}");
                NewDebouncerFailed.send(&app_clone, is_desktop_notify);
                return Err(());
            }
        };

        // 「サブフォルダを含まない」の反映
        let mode = if config.recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };

        // フォルダを監視対象に登録 (NonRecursive = サブフォルダを含まない)
        if let Err(error) = debouncer.watch(watch_path_clone, mode) {
            eprintln!("{error}");
            DebounceStartFailed.send(&app_clone, is_desktop_notify);
            return Err(());
        };

        // デバウンサをフィールドに保持
        *lock_debouncer = Some(debouncer);

        // フォルダ監視開始の成功を通知
        Success.send(&app_clone, is_desktop_notify);
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