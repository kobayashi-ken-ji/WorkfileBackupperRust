use std::path::{Path, PathBuf};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tokio::task::JoinSet;
use tokio::runtime;
use futures::future::BoxFuture;

use crate::utilities::{ResutlErrPrint, lock_mutex};

//=============================================================================
// 本番/モック の切替え
//=============================================================================

/// 本番・モックを切替え
pub enum FileManager {
    Real(ActiveFileManager),
    Mock(MockFileManager),
}

impl FileManager {

    pub fn execute<F>(&self, path: &Path, callback: F)
    where F: FnOnce(PathBuf) -> BoxFuture<'static, ()> + Send + 'static
    {
        match self {
            Self::Real(manager) => manager.execute(path, callback),
            Self::Mock(manager) => manager.execute(path, callback),
        }
    }

    pub async fn join_tasks(&self) {
        match self {
            Self::Real(manager) => manager.join_tasks().await,
            Self::Mock(manager) => manager.join_tasks().await,
        }
    }
}


/// ActiveFileManager のテスト用モック
pub struct MockFileManager {
    pub paths: Arc<Mutex<Vec<PathBuf>>>,
}

impl MockFileManager {

    pub fn new() -> Self {
        Self { paths: Arc::new(Mutex::new(Vec::new())) }
    }


    /// パスの記録のみを行う (callbackは実行しない)
    pub fn execute<F>(&self, path: &Path, _callback: F)
    where F: FnOnce(PathBuf) -> BoxFuture<'static, ()> + Send + 'static
    {
        let mut paths = lock_mutex(&self.paths);
        paths.push(path.into());
    }


    /// 何も行わず、即終了する
    pub async fn join_tasks(&self) {}
}

//=============================================================================
// ファイルマネージャー本体
//=============================================================================

/// 処理中のファイルを管理
/// 
/// 主な機能
/// * 新規タスク上で指定した処理を実行
/// * 同じファイルへの処理重複を回避
/// * 停止操作時に、全処理の終了を待機
pub struct ActiveFileManager  {

    // ※ std版のMutex
    /// 処理中ファイルのリスト
    active_files: Arc<Mutex< HashSet<PathBuf> >>,

    // ※ Tokio版のMutex (lockしたままawaitさせる必要があるため)
    /// 生成した非同期タスクのリスト
    tasks: tokio::sync::Mutex<JoinSet<()>>,

    // Tokioのランタイムハンドル
    handle: runtime::Handle,
}

impl ActiveFileManager  {

    /// コンストラクタ
    pub fn new(handle: tokio::runtime::Handle) -> Self {
        Self {
            active_files: Arc::new(Mutex::new(HashSet::new())),
            tasks: tokio::sync::Mutex::new(JoinSet::new()),
            handle: handle,

            // Tauri以外で使わないのなら、以下でも可能
            // handle: tauri::async_runtime::handle().inner().clone(),
        }
    }

    /*
        スレッドへ非同期処理を渡す為に必要な指定

            FnOnce  : 所有権を消費するため必要
            Send    : スレッド間でデータを渡せる
            'static : スレッドより長生き (参照を含まない、など)

            BoxFuture:
                FutureをBoxで包み、実体をヒープ領域に固定する
                ・所有権とライフタイムを確保
                ・サイズを一律化
     */

    /// 新規スレッド上で処理を実行
    /// 
    /// ファイルパスをリストに登録し、処理を実行する。 
    /// 処理が終了すると登録が解除される。 
    /// 指定したファイルパスが登録中の場合は、処理をスキップする。 
    /// 
    /// # 引数
    /// * `path` - 処理中リストに登録するファイルパス
    /// * `callback` - 実行する処理 (async)
    /// 
    /// # パニック
    /// async内でこのメソッドを呼び出すとパニックが発生する。 
    /// tokio::sync::Mutex::blocking_lock() を実行するため、デッドロックの可能性がある。 
    pub fn execute<F>(&self, path: &Path, callback: F)
    where F: FnOnce(PathBuf) -> BoxFuture<'static, ()> + Send + 'static
    {
        // 処理中ファイルのリストを排他ロックする
        let files = self.active_files.clone();
        let mut lock_files = lock_mutex(&files);

        // ファイルが既に処理中の場合はスキップ
        if lock_files.contains(path) {
            // println!("既に処理中のためスキップ");
            return;
        }

        // 処理中リストに追加し、即座にロックを解除
        lock_files.insert(path.to_path_buf());
        drop(lock_files);

        // clone + move でライフタイムを保証
        let path = path.to_path_buf();

        // タスクリストをロック
        let mut tasks_lock = self.tasks.blocking_lock();

        // タスク数が増えた場合は、1つ終了するまで待機
        // 追加しない為の待機なので、Mutexやデバウンサーをブロックしていても問題なし
        // [!] join_next()を行わないと、タスクが終了していてもメモリ上に残り続けてしまう
        const MAX_CONCURRENCY: usize = 8;
        if tasks_lock.len() >= MAX_CONCURRENCY {
            self.handle.block_on(async {
                if let Some(result) = tasks_lock.join_next().await {
                    result.eprint("ファイル書込待機タスクの終了待ちに失敗");
                };
            });
        }

        // JoinSetに非同期タスクを渡す
        tasks_lock.spawn_on(
            async move {
                // ガードを生成
                // スレッド終了時、自動で処理中リストからファイルを削除
                // スレッド内でパニックが発生しても実行される
                let _guard = FileTaskGuard {
                    path: path.clone(),
                    active_files: files,
                };

                // ファイルの書込終了まで待機し、バックアップを実行
                callback(path).await;
            },

            // Tauri側のTokioランタイムで実行させる
            &self.handle
        );
    }


    /// 登録されている処理全てが終了するまで待機
    pub async fn join_tasks(&self) {

        // タスクリストをロック
        // Tokio版のため、ポイズンエラーは発生しない
        let mut tasks_lock = self.tasks.lock().await;

        // 終了したタスクから順にSomeを返され、すべて終わるまで非同期に待機する
        while let Some(_) = tasks_lock.join_next().await {}
            // join_next()
            // いずれかのタスクの完了を待機する
            // JoinSet内が空の場合はNoneが返る
    }
}

//=============================================================================
// RAII用ガード
//=============================================================================

/// スコープを抜けた際に、自動で処理中リストからファイルを削除する
struct FileTaskGuard {
    path: PathBuf,
    active_files: Arc<Mutex<  HashSet<PathBuf>  >>,
}

impl Drop for FileTaskGuard {
    fn drop(&mut self) {

        // 処理中リストをロックし、リストからファイルを削除
        let mut lock_files = lock_mutex(&self.active_files);
        lock_files.remove(&self.path);
    }
}

//=============================================================================
// テスト
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use std::thread::sleep as std_sleep;
    use tokio::{time::sleep};
    use futures::FutureExt;

    #[test]
    fn test_active_file_manager() {

        // Tokioランタイムを生成
        let rantaime = tokio::runtime::Runtime::new().unwrap();
        let tokio_handle = rantaime.handle().clone();

        // インスタンスを生成
        let manager = ActiveFileManager::new(tokio_handle);

        // テスト処理の時間 / テスト処理が終わるのを待機するための時間
        const EXECUTE_DURATION : Duration = Duration::from_secs(1);
        const WAIT_DURATION    : Duration = Duration::from_secs(2);

        /// テスト実行・検証
        /// 
        /// 「1秒間待機する」処理がexecuteに渡される。
        /// 
        /// # 引数
        /// * `manager` - テスト用インスタンス
        /// * `path` - 登録するファイル名
        /// * `expected_length` - 期待されるファイル登録数
        fn test(manager: &ActiveFileManager, path: &str, expected_length: usize) {

            // [!] 非同期上だとパニックが発生
            // execute内部でblocking_lockを使用しているため

            // テスト実行
            let path = Path::new(path);
            manager.execute(path, move |path| {
                async move {
                    sleep(EXECUTE_DURATION).await;
                    // println!("処理終了: {:?}", path);
                }.boxed()
            });

            // タスク数を検証 (期待値以上」かを検証)
            //      [!] TokioのMutexは、処理が終わっても len() 値は減らない
            //      join_next().await で結果を取り出すと減る
            let result_length = manager.tasks.blocking_lock().len();
            assert!(result_length >= expected_length, "処理中のタスク数");
            // println!("処理中のタスク数: {result_length}, {expected_length}");

            // ファイル数を検証 (処理が終わり次第、len() 値が減る)
            let active_files = lock_mutex(&manager.active_files);
            let result_length = active_files.len();
            assert_eq!(result_length, expected_length, "処理中のファイル数");
            // println!("処理中のファイル数: {result_length}, {expected_length}");
        
            // ファイル名が登録されているか検証
            assert!(active_files.contains(path), "処理中のファイル名:\n{:?}", path);
            // println!("処理中のファイル名:{:?}\n", path);
        }

        // テストケースを定義、実行
        test(&manager, "test1.txt", 1);     // 登録+1 → 1
        test(&manager, "test1.txt", 1);     // 登録済み → 登録数は増えない
        test(&manager, "test2.txt", 2);     // 登録+1 → 2
        std_sleep(WAIT_DURATION);           // 処理終了を待機 → 0
        test(&manager, "test3.txt", 1);     // 登録+1 → 1

        // async内で停止処理
        // ブロッキング処理のため、タスク終了を待ってからテスト関数が終わる
        rantaime.block_on(async move {

            // 停止を待機
            manager.join_tasks().await;

            // 処理中ファイルが0になったかを確認
            let length = manager.tasks.lock().await.len();
            assert_eq!(length, 0, "全ファイルの処理終了を確認");
            // println!("全ファイルの処理終了を確認: {}, {}\n", length, 0);
        });
    }
}