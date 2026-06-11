use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use futures::FutureExt;
use notify_debouncer_full::{DebounceEventHandler, DebounceEventResult, Debouncer, RecommendedCache, new_debouncer, notify::*};
use tokio::sync::mpsc::Sender;  // Tokio版を使用すること

use crate::models::config::Config;
use crate::models::notify::{Notifier, StartResult, StopResult, WaitResult, WatchInfo};
use crate::services::file_manager::{ActiveFileManager, FileManager};
use crate::services::target_checker::TargetChecker;
use crate::services::wait::wait_for_file_writing;
use crate::services::backup;
use crate::services::timer;
use crate::utilities::{ResutlErrPrint, lock_mutex};

//=============================================================================
// ロジック部分
//=============================================================================

/// フォルダ内の変化を検出後の処理を担当
pub struct BackupEventHandler {
    file_manager: Arc<FileManager>,
    target_checker: TargetChecker,
    notifier: Notifier,
    timer_tx: Option<Sender<()>>,
    source_path: PathBuf,
    destination_path: PathBuf,
}

// new_debouncer() の引数に渡すためにトレイトを実装
impl DebounceEventHandler for BackupEventHandler {
    fn handle_event(&mut self, event: DebounceEventResult) {
        self.process_event(event);
    }
}

impl BackupEventHandler {

    // デバウンサー検出後のロジック
    pub fn process_event(&self, result: DebounceEventResult) {
        
        let notifier = &self.notifier;

        // デバウンサー検出エラーの処理
        let events = match result {
            Ok(events) => events,
            Err(errors) => {
                eprintln!("{:#?}", errors);
                notifier.notify(&WatchInfo::DebounceError);
                return;
            }
        };

        // フォルダ内で発生した全てのイベント
        for debounced_event in events {
            let kind = debounced_event.event.kind;

            // ファイルの「修正・変更・新規作成」に絞る
            let is_modify = kind.is_modify() || kind.is_create();
            if !is_modify { continue; }

            // 検出したファイル全てに処理
            for path in debounced_event.event.paths {

                // バックアップ対象かを判定
                if !self.target_checker.is_target(&path) {
                    notifier.notify( &WatchInfo::NotTarget(path) );
                    continue;
                }

                // 変更検出を通知
                notifier.notify( &WatchInfo::Detected(path.clone()) );

                // ファイル変更終了待ち + バックアップ処理
                self.wait_and_backup(path);
            }
        }
    }


    // ファイル変更終了待ち + バックアップ処理
    fn wait_and_backup(&self, path: PathBuf) {

        // move用に複製
        let notifier         = self.notifier.clone();
        let timer_tx         = self.timer_tx.clone();
        let source_path      = self.source_path.clone();
        let destination_path = self.destination_path.clone();

        // 「処理中ファイルリスト」に登録する
        self.file_manager.execute(&path, move |path| {

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
                let result = backup::back_up_file(&source_path, &destination_path, &path);
                notifier.notify(&result);
                
                // 未保存時間をリセット
                // None → 通知OFFの状態
                if let Some(timer_tx) = timer_tx {
                    timer_tx.send(()).await.eprint("未保存時間リセット信号の送信エラー");
                }

            }.boxed()
        });
    }
}

//=============================================================================
// デバウンサーの生成
//=============================================================================

use std::path::Path;

/// デバウンサーの開始処理
pub fn start_debouncer<H>(source_path: &Path, recursive: bool, notifier: Notifier, handler: H)
    -> core::result::Result<Debouncer<RecommendedWatcher, RecommendedCache>, ()>
where H: DebounceEventHandler,
{
    use StartResult::*;

    // デバウンサを生成
    // フォルダ内を監視し、変更発生時に引数関数を実行する
    // 指定時間以上イベント通知が途切れるのを待つ
    let mut debouncer = new_debouncer(Duration::from_secs(5), None, handler)
        .map_err(|error| {
            eprintln!("{error}");
            notifier.notify(&NewDebouncerFailed);
            ()
        })?;

    // デバウンサ生成のエラー処理
    // let mut debouncer = match debouncer {
    //     Ok(debouncer) => debouncer,
    //     Err(error) => {
    //         eprintln!("{error}");
    //         notifier.notify(&NewDebouncerFailed);
    //         return Err(());
    //     }
    // };

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

    // フォルダ監視開始の成功を通知
    notifier.notify(&Success);

    Ok(debouncer)
}

//=============================================================================
// フォルダ監視の 開始/停止 処理
//=============================================================================

pub struct AppManager {
    file_manager: Arc<FileManager>,

    // TauriのStateへの登録は固定のまま、デバウンサだけ使い捨てにする
    // Optionが Ok→稼働中、None→停止中
    debouncer: Mutex<Option<Debouncer<RecommendedWatcher, RecommendedCache>>>,
}

impl AppManager {

    /// コンストラクタ
    pub fn new(tokio_handle: &tokio::runtime::Handle) -> Self {

        // 本番用を生成
        let file_manager = 
            FileManager::Real(ActiveFileManager::new(tokio_handle.clone()));

        Self {
            file_manager: Arc::new(file_manager),
            debouncer: Mutex::new(None),
        }
    }


    /// Tauriのコマンドから呼ばれる、開始処理
    pub fn start(
        &self, config: Config, 
        notifier: Notifier, unsaved_notifier: Notifier
    ) -> core::result::Result<(), ()> {

        use StartResult::*;

        // 「ファイル未保存時間」の計測用スレッド作成
        // 計測しない場合は None
        let timer_tx = if config.is_notify_unsaved {
            let tx = timer::run_timer(config.notify_interval, unsaved_notifier);
            Some(tx)
        } else {
            None
        };

        // 「ファイルがバックアップ対象か」の判定機
        let target_checker = TargetChecker::new(
            config.all_files_enabled,
            &config.extensions
        );

        // デバウンサーに渡す構造体を生成
        let handler = BackupEventHandler {
            file_manager: self.file_manager.clone(),
            target_checker,
            notifier: notifier.clone(),
            timer_tx,
            source_path: config.source_path.clone(),
            destination_path: config.destination_path.clone(),
        };

        //-------------------------------------------------
        
        // 処理中ファイルのリストを排他ロックする
        let mut lock_debouncer = lock_mutex(&self.debouncer);

        // 既に開始していたら終了 (二重起動防止)
        if lock_debouncer.is_some() {
            notifier.notify(&AlreadyRunning);
            return Err(());
        }

        // デバウンサーを生成
        let debouncer = start_debouncer(
            &config.source_path, config.recursive, notifier, handler)?;

        // デバウンサをフィールドに格納
        *lock_debouncer = Some(debouncer);
        Ok(())
    }


    /// Tauriのコマンドから呼ばれる、停止処理
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

//=============================================================================
// テスト
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use notify_debouncer_full::{DebouncedEvent, notify::{Event, EventKind, event}};
    use crate::{models::notify::MockNotifier, services::{file_manager, timer::run_timer}};
    use crate::services::file_manager::MockFileManager;

    // デバウンサーが検知後の処理をテスト
    #[tokio::test]
    async fn test_debouncer_callback_logic() {

        //-------------------------------------------------
        // テストケースを作成
        //-------------------------------------------------

        // バックアップ対象の拡張子
        let extensions: Vec<String> = vec![
            "txt".into(),
        ];

        // タプルの内容
        //  0: ファイル名
        //  1: 実際にディスクに作成するか (falseなら フォルダor存在しないファイル)
        //  2: 期待されるバックアップ件数
        let cases = [
            ("test.txt",        true,  1),  // 正常
            ("deep/dir/a.txt",  true,  1),  // 深い階層 (テスト開始時に親フォルダを作成)
            ("IMAGE.TXT",       true,  1),  // 大文字
            ("no_ext",          true,  0),  // 拡張子なし
            (".gitignore",      true,  0),  // 隠しファイル
            ("some_folder",     false, 0),  // フォルダ (対象外のため、作成しないことで再現)
        ];

        //-------------------------------------------------
        // 共通準備
        //-------------------------------------------------

        // OSの安全な場所に、テスト専用一時フォルダを作成
        let tmp_dir = tempfile::tempdir().unwrap();

        // 各パスを生成
        let source_path      = tmp_dir.path().join("src");
        let destination_path = tmp_dir.path().join("dest");   // モック部分のため出力されない
        let test_path        = source_path.join("test.txt");

        // コピー元/先フォルダを作成
        fs::create_dir_all(&source_path).unwrap();
        fs::create_dir_all(&destination_path).unwrap();

        // ファイルを作成
        std::fs::File::create(&test_path).unwrap();

        // モックを生成し、Arcをクローンしておく
        let mock = MockFileManager::new();
        let paths = mock.paths.clone();
        
        // デバウンサーに渡す構造体(モック)を生成
        let handler = BackupEventHandler {
            file_manager     : Arc::new(FileManager::Mock(mock)),
            target_checker   : TargetChecker::new(false, &extensions),
            notifier         : Notifier::Mock(MockNotifier::new()),
            timer_tx         : Some(run_timer(10, Notifier::Mock(MockNotifier::new()))),
            source_path      : source_path.clone(),
            destination_path : destination_path.clone(),
        };
        
        //-------------------------------------------------
        // ケースごとのテスト
        //-------------------------------------------------

        for (file_name, is_file, expected_count) in cases {
            let full_path = source_path.join(file_name);

            if is_file {
                // 親フォルダが無ければ生成し、ファイルを生成
                fs::create_dir_all(&full_path.parent().unwrap()).unwrap();
                std::fs::File::create(&full_path).unwrap();

            } else {
                // フォルダを作成
                fs::create_dir_all(&full_path).unwrap();
            }

            // ダミーの変更イベントを組み立てる
            let raw_event = Event::new(EventKind::Modify(event::ModifyKind::Any));
            let mut debouncer_event = DebouncedEvent::new(raw_event, std::time::Instant::now());

            // 手動でパスを追加
            debouncer_event.event.paths = vec![full_path.clone()];
            let result: DebounceEventResult = Ok(vec![debouncer_event]);
            // println!("{:?}", result);

            // テストを実行
            // ※モックのため、FileManager::execute() の引数クロージャは実行されない
            handler.process_event(result);

            //-------------------------------------------------
            // 結果を検証
            //-------------------------------------------------

            // file_managerに1件追加されたか検証
            let mut paths = lock_mutex(&paths);

            // バックアップ件数が期待通りか
            assert_eq!(paths.len(), expected_count, "{:?}", full_path);

            // パス名が等しいか
            if paths.len() > 0 {
                assert_eq!(paths[0], full_path);
            }
            
            // println!("{}", paths.len());
            // println!("{:?}", paths);

            // 次のテストのために空にする
            paths.clear();
        }
    }
}