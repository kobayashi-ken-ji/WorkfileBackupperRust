//! アプリの主機能(フォルダ監視・バックアップ)の開始・停止を担当

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use futures::FutureExt;
use tokio::sync::mpsc::Sender;  // Tokio版を使用すること
use notify_debouncer_full::{
    DebounceEventHandler, DebounceEventResult, Debouncer,
    RecommendedCache, new_debouncer,
    notify::{RecommendedWatcher, RecursiveMode}
};

use crate::models::config::Config;
use crate::models::notify::{Notifier, StartResult, StopResult, WaitResult, WatchInfo};
use crate::services::file_manager::{ActiveFileManager, FileManager};
use crate::services::target_checker::TargetChecker;
use crate::services::wait::wait_for_file_writing;
use crate::services::backup;
use crate::services::timer;
use crate::utilities::{ResutlErrPrint, SafeMutex};

//=============================================================================
// new_debouncer() の引数
//=============================================================================

/// 「監視フォルダの変化を検出」した後の処理を担当
/// 
/// new_debouncer() の引数に渡す構造体。 
/// バックアップ対象かを判別し、ファイル書込終了を待機し、バックアップを行う。 
pub struct BackupEventHandler {
    file_manager: Arc<FileManager>,  // 処理中のファイルとタスクを管理
    target_checker: TargetChecker,   // バックアップ対象かの判別機
    notifier: Notifier,              // 情報送信機 (デスクトップ・ログ・コンソールへ)
    timer_tx: Option<Sender<()>>,    // 「ファイル未保存時間」をリセットするための送信機
    source_path: PathBuf,            // バックアップ元フォルダ (監視するフォルダ)
    destination_path: PathBuf,       // バックアップ先フォルダ
}

// new_debouncer() の引数に渡すためにトレイトを実装
impl DebounceEventHandler for BackupEventHandler {
    fn handle_event(&mut self, event: DebounceEventResult) {
        self.process_event(event);
    }
}

impl BackupEventHandler {

    /// デバウンサー検出後のロジック
    /// デバウンサーリザルトからファイルパスを抽出し、別タスクへ渡す
    fn process_event(&self, result: DebounceEventResult) {
        
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

                // ファイル書込み終了待ち + バックアップ処理
                self.wait_and_backup(path);
            }
        }
    }


    /// ファイル書込み終了待ち + バックアップ処理
    /// タスクを生成し、その中で非同期に実行する。 
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
// フォルダ監視の 開始/停止 処理
//=============================================================================

/// フォルダ監視の 開始/停止 を行う
///
/// アプリ主機能の 有効化/無効化 を切替える、統合的な処理を担当。
/// TauriのStateに登録され、command から実行される。
pub struct AppManager {

    /// 処理中のファイルとタスクを管理
    file_manager: Arc<FileManager>,

    /// Optionが Ok→稼働中、None→停止中
    /// TauriのStateへの登録は固定のまま、デバウンサだけ使い捨てにする 
    debouncer: Mutex<Option<Debouncer<RecommendedWatcher, RecommendedCache>>>,
}

impl AppManager {

    /// コンストラクタ 
    /// 
    /// # 引数
    /// * `tokio_handle` - Tauri側のランタイムを渡す (重複生成をさけるため)
    pub fn new(tokio_handle: &tokio::runtime::Handle) -> Self {

        let file_manager =
            FileManager::Real(ActiveFileManager::new(tokio_handle.clone()));

        Self {
            file_manager: Arc::new(file_manager),
            debouncer: Mutex::new(None),
        }
    }


    /// フォルダ監視を開始
    /// 
    /// Tauriのコマンドから呼び出される。 
    /// 監視を停止するには stop() を実行する。 
    /// 
    /// # 引数
    /// * `config` - ユーザー設定の値
    /// * `notifier` - バックアップ関連の通知機
    /// * `unsaved_notifier` - ファイル未保存関連の通知機
    /// 
    pub fn start(
        &self, config: Config, notifier: Notifier, unsaved_notifier: Notifier
    ) -> Result<(), ()> {

        // 「ファイル未保存時間」の計測用スレッド作成
        // 計測しない場合は None
        let timer_tx = if config.is_notify_unsaved {
            let tx = timer::run_timer(config.notify_interval as u64, unsaved_notifier);
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
        let mut lock_debouncer = self.debouncer.safe_lock();

        // 既に開始していたら終了 (二重起動防止)
        if lock_debouncer.is_some() {
            notifier.notify(&StartResult::AlreadyRunning);
            return Err(());
        }

        // デバウンサーを生成
        let debouncer = Self::start_debouncer(
            &config.source_path, config.recursive, notifier, handler)?;

        // デバウンサをフィールドに格納
        *lock_debouncer = Some(debouncer);
        Ok(())
    }


    /// デバウンサーを生成・開始
    fn start_debouncer<H>(
        source_path: &Path, recursive: bool, notifier: Notifier, handler: H
    ) -> Result<Debouncer<RecommendedWatcher, RecommendedCache>, ()>
    where H: DebounceEventHandler,
    {
        use StartResult::*;

        // デバウンサを生成
        // フォルダ内を監視し、変更発生時に引数関数を実行する。
        // 指定時間以上イベント通知が途切れるのを待つ。
        let debouncer = new_debouncer(Duration::from_secs(5), None, handler);

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

        // フォルダ監視開始の成功を通知
        notifier.notify(&Success);

        Ok(debouncer)
    }


    /// フォルダ監視を停止
    /// 
    /// Tauriのコマンドから呼び出される。 
    /// 既に停止中の場合は処理をスキップする。 
    /// 停止完了を待機するために await が必要。 
    /// 
    /// # 戻り値
    /// 停止処理の結果
    /// 
    pub async fn stop(&self) -> StopResult {

        // std版Mutexは、ロック状態では await を使用できないため、
        // ブロックでドロップさせている
        {
            // ロック解除 + ポイズンエラー処理
            let mut lock_debouncer = self.debouncer.safe_lock();

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
    use crate::models::notify::{BackupResult, MockNotifier, ToNotify};
    use crate::services::file_manager::MockFileManager;

    // デバウンサー検知後の処理をテスト
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
        let file_manager_log = mock.log.clone();
        
        // デバウンサーに渡す構造体(モック)を生成
        let handler = BackupEventHandler {
            file_manager     : Arc::new(FileManager::Mock(mock)),
            target_checker   : TargetChecker::new(false, &extensions),
            notifier         : Notifier::Mock(MockNotifier::new()),
            timer_tx         : None,
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
            let mut log = file_manager_log.safe_lock();

            // バックアップ件数が期待通りか
            assert_eq!(log.len(), expected_count, "バックアップ件数が不一致: {:?}", full_path);

            // パス名が等しいか
            if log.len() > 0 {
                assert_eq!(log[0], full_path);
            }
            
            // println!("{}", log.len());
            // println!("{:?}", log);

            // 次のテストのために空にする
            log.clear();
        }
    }


    /// AppManager全体のテスト
    /// 
    /// アプリ主機能の統合的なテストを行う。 
    /// (フォルダ監視開始・書込み検出時のバックアップ・監視停止)
    #[test]
    fn test_app_manager() {

        // Tokioランタイムを生成
        let rantaime = tokio::runtime::Runtime::new().unwrap();
        let tokio_handle = rantaime.handle().clone();

        //-------------------------------------------------
        // 環境構築
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

        //-------------------------------------------------
        // 共通インスタンス生成
        //-------------------------------------------------

        // インスタンスを生成
        let manager = AppManager::new(&tokio_handle);

        // ユーザー設定を生成
        let mut default_config = Config::default();
        default_config.source_path = source_path.clone();
        default_config.destination_path = destination_path.clone();
        default_config.extensions = vec!["txt".into()];

        // 通知機 / 通知ログへの参照 を生成

        // バックアップ関連通知
        let notifier = MockNotifier::new();
        let notifier_log = notifier.log.clone();
        let notifier = Notifier::Mock(notifier);

        // ファイル未保存関連の通知
        let unsaved_notifier = MockNotifier::new();
        let unsaved_notifier = Notifier::Mock(unsaved_notifier);

        //-------------------------------------------------
        // フォルダ監視開始の失敗テスト
        //-------------------------------------------------

        use crate::models::notify::NotifyPayload;

        // 各テスト共通の引数
        let args = (&manager, &notifier, &unsaved_notifier, &notifier_log);

        /// フォルダ監視開始テスト 共通処理
        /// 
        /// # 引数
        /// * `comment` - 失敗時に表示されるコメント
        /// * `args` - 各テスト共通の引数
        /// * `config` - ユーザー設定
        /// * `expected_return` - 期待される start() の戻り値
        /// * `expected_notify` - 期待される notifier で送信される値
        fn test_start(
            comment: &str,
            args: &(&AppManager, &Notifier, &Notifier, &Arc<Mutex<Vec<NotifyPayload>>>),
            config: Config,
            expected_return: Result<(), ()>,
            expected_notify: StartResult,
        ) {
            // 共通の引数を展開
            let &(manager, notifier, unsaved_notifier, notifier_log) = args;

            // テスト実行
            let result = manager.start(
                config, notifier.clone(), unsaved_notifier.clone());

            // 通知された値を取得
            let log_payload = notifier_log.safe_lock().last().unwrap().clone();

            // 検証 (戻り値・通知値)
            assert_eq!(&result, &expected_return, "{comment}");
            assert_eq!(&log_payload, &expected_notify.to_payload(), "{comment}");
        }

        // 存在しないフォルダを指定
        let mut invalid_config = default_config.clone();
        invalid_config.source_path = tmp_dir.path().join("not_exist");

        // テストケースの定義と実行
        {
            use StartResult::*;

            test_start(
                "デバウンサーが監視開始に失敗するテスト",
                &args, invalid_config, Err(()), DebounceStartFailed
            );

            test_start(
                "フォルダ監視の開始に成功するテスト",
                &args, default_config.clone(), Ok(()), Success
            );
            
            test_start(
                "フォルダ監視が既に開始中の場合のテスト",
                &args, default_config.clone(), Err(()), AlreadyRunning
            );
        }

        //-------------------------------------------------
        // ファイルが変更されたときの挙動テスト
        //-------------------------------------------------

        // 新規ファイルを生成
        let path = source_path.join("new_file.txt");
        std::fs::File::create(&path).unwrap();

        // 「検出・待機・バックアップ」が終わるまで待機
        std::thread::sleep(Duration::from_secs(15));

        // 通知された値を取得
        let comment = "フォルダ監視中にファイル変更を発生させるテスト";
        let log_payload = notifier_log.safe_lock().last().unwrap().clone();
        let expected = BackupResult::Copied("".into()).to_payload();

        // バックアップされたファイル名にはタイムスタンプが付与されるため、概要文で比較
        assert_eq!(&log_payload.title, &expected.title, "{comment}");
        // println!("{comment}\n{}\n{}\n", &log_payload.title, &expected.title);      

        // ファイルが存在するか検証
        // ※ bodyにはファイル名のみが入っている (サブフォルダ名も除去)
        let backup_path = destination_path.join(log_payload.body);
        assert!(backup_path.is_file(), "{comment}: {:?}", backup_path);
        // println!("{comment}: {:?}", backup_path);

        //-------------------------------------------------
        // フォルダ監視停止のテスト
        //-------------------------------------------------

        // タスクを実行/終了待機
        rantaime.block_on(async move {
            use StopResult::*;

            // 上記テストで、既に監視が開始されている
            let comment = "フォルダ監視が正常停止するテスト";
            let result = manager.stop().await;
            assert_eq!(result, Success, "{comment}");

            let comment = "フォルダ監視が既に停止中のテスト";
            let result = manager.stop().await;
            assert_eq!(result, AlreadyStopped, "{comment}");
        });
    }
}