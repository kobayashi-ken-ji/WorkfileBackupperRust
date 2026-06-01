use std::path::{Path, PathBuf};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tokio::task::JoinSet;
use tokio::runtime;
use futures::future::BoxFuture;

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
        if let Ok(mut lock_files) = self.active_files.lock() {
            lock_files.remove(&self.path);
        }
    }
}

//=============================================================================
// 本体
//=============================================================================

/// 処理中のファイルを管理
pub struct ActiveFileManager  {

    // ※ std版のMutex
    /// 処理中ファイルのリスト (所有者複数化、排他制御化)
    active_files: Arc<Mutex<  HashSet<PathBuf>  >>,

    // ※ Tokio版のMutex
    /// 生成した非同期タスクのリスト
    tasks: tokio::sync::Mutex<JoinSet<()>>,

    // Tokioのランタイムハンドル
    handle: runtime::Handle,
}

impl ActiveFileManager  {

    /// 別スレッドがパニックするとロックが汚染される。中の値を強制取得して続行。
    const MUTEX_POISON_ERR: &str
        = "ミューテックスのポイズンエラーが発生。強制取得して続行します。";

    /// コンストラクタ
    pub fn new(handle: runtime::Handle) -> Self {
        Self {
            active_files: Arc::new(Mutex::new(HashSet::new())),
            tasks: tokio::sync::Mutex::new(JoinSet::new()),
            handle: handle,

            // Tauri以外で使わないのなら、以下でも可能
            // handle: tauri::async_runtime::handle().inner().clone(),

            // 現在動いているTauri（Tokio）のランタイムのハンドルを捕まえる
            // ※ Tauriではエラーとなった
            // handle: runtime::Handle::current(), 
        }
    }


    /// 新規スレッド上で指定処理を実行 (ファイルが既に実行中の場合は処理をスキップ)
    pub fn execute<F>(&self, path: &Path, callback: F)
    where
        // クロージャでmoveするために必要なトレイト境界
        //      FnOnce   : 所有権を消費するため必要
        //      Send     : スレッド間でデータを渡せる
        //      'static  : スレッドより長生き (参照を含まない、など)
        //      BoxFuture: async処理は固有型のため、Boxで包んでいる
        F: FnOnce(PathBuf) -> BoxFuture<'static, ()> + Send + 'static,
    {
        let files = self.active_files.clone();

        // 処理中ファイルのリストを排他ロックする
        let mut lock_files = match files.lock() {
            Ok(guard) => guard,
            Err(poison_err) => {
                println!("{}", Self::MUTEX_POISON_ERR);
                poison_err.into_inner()
            }
        };

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

        // 終了したタスクから順にポップされ、すべて終わるまで非同期に待機する
        while let Some(_) = tasks_lock.join_next().await {}
            // join_next()
            // いずれかのタスクの完了を待機する
            // JoinSet内が空の場合はNoneが返る
    }
}