use std::path::{Path, PathBuf};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tokio::task::JoinSet;
use tokio::runtime;
use futures::future::BoxFuture;

use crate::utilities::lock_mutex;

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
// 本番/モック の切替え
//=============================================================================

/// 本番・モックを切替え (トレイトの代用)
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
// 本体
//=============================================================================

/// 処理中のファイルを管理
pub struct ActiveFileManager  {

    // ※ std版のMutex (lock中にawaitが不要なため、軽量なstd版)
    /// 処理中ファイルのリスト (所有者複数化、排他制御化)
    active_files: Arc<Mutex<  HashSet<PathBuf>  >>,

    // ※ Tokio版のMutex (lockしたままawaitさせる必要があるため)
    /// 生成した非同期タスクのリスト
    tasks: tokio::sync::Mutex<JoinSet<()>>,

    // Tokioのランタイムハンドル
    handle: runtime::Handle,
}

impl ActiveFileManager  {

    /// コンストラクタ
    pub fn new(handle: runtime::Handle) -> Self {
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
    /// 新規スレッド上で指定処理を実行 (ファイルが既に実行中の場合は処理をスキップ)
    pub fn execute<F>(&self, path: &Path, callback: F)
    where F: FnOnce(PathBuf) -> BoxFuture<'static, ()> + Send + 'static
    {
        // 処理中ファイルのリストを排他ロックする
        let files = self.active_files.clone();
        let mut lock_files = lock_mutex(&files);

        // ファイルが既に処理中の場合はスキップ
        if lock_files.contains(path) { return; }

        // 処理中リストに追加し、即座にロックを解除
        lock_files.insert(path.to_path_buf());
        drop(lock_files);

        // clone + move でライフタイムを保証
        let path = path.to_path_buf();

        // タスクリストをロック
        // 処理をブロックするため、タスクだけ作り、すぐに解放
        let mut tasks_lock = self.tasks.blocking_lock();

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


    /// 全ての非同期タスクが終了するまで待機 (シャットダウン用)
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
