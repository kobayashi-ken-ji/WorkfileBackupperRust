use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::thread;
use std::time::Duration;
use tauri_plugin_notification::NotificationExt;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager,
    Emitter,
};
use tokio::sync::watch;

use crate::MessageSender;
use crate::services::{self, watch_manager::WatchManager};


//=============================================================================
// PathBuf周りの注意点
//=============================================================================

fn test_path_buf(path: String) -> Result<(), String> {

    // String → PathBuf 変換
    let path_buf = PathBuf::from(&path);
    println!("有効なパスか: {}", path_buf.exists());

    // PathBuf → String 変換
    // ※UTF-8として不正な部分は「」に変換する
    let _path = path_buf.to_string_lossy().into_owned();

    // 正規化 (絶対パス化 + 余計な/や.を削除)
    // Windowsでは、UNC/拡張接頭辞(\\?\) を付与 (260文字制限を解決)
    let canonical_path = path_buf.canonicalize()
        .map_err(|e| format!("パスが正しくない、または存在しない: {e}"))?;

    Ok(())
}


// 拡張接頭辞を外す (UI表示用)
fn clean_path_for_ui(path: &Path) -> String {

    let path_str = path.to_string_lossy();

    // Windows環境のみ拡張接頭辞を外す
    #[cfg(windows)]
    if path_str.starts_with(r"\\?\") {
        return path_str[4..].to_string()
    }

    path_str.into_owned()
}

//=============================================================================

// 画面から呼び出される関数
// 引数に tauri::AppHandle を追加すると、自動で渡してくれる
#[tauri::command]
pub fn start_backup(
    sender:tauri::State<'_, MessageSender>, manager: tauri::State<'_, WatchManager>,
    app: tauri::AppHandle, path: String, extension: String) -> Result<String, String> {

    // アプリの処理を記述
    let tx = sender.tx.clone();
    manager.start(&PathBuf::from(&path), tx).unwrap();

    // let tx = state.tx.clone();
    // tauri::async_runtime::spawn(async move {
    //     services::watch::run(tx).unwrap();
    // });

    // println!("開始します");

    // デスクトップ通知を送信
    // ※ インストーラを使用しない場合、アプリ名がPowerShellになる
    app.notification()
        .builder()
        .title("バックアップを開始しました")
        .body(&path)
        .show()
        .unwrap();

    // 画面へのレスポンス
    Ok(format!("バックアップを開始しました"))
}


#[tauri::command]
pub async fn stop_watch(
    sender:tauri::State<'_, MessageSender>, manager: tauri::State<'_, WatchManager>,
    app: tauri::AppHandle, path: String, extension: String) -> Result<String, String> {

    // アプリの処理を記述
    manager.stop(&PathBuf::from(&path)).await;

    // デスクトップ通知を送信
    app.notification()
        .builder()
        .title("バックアップを停止しました")
        .body(&path)
        .show()
        .unwrap();

    // 画面へのレスポンス
    Ok(format!("バックアップを停止しました"))
}



// 開始ボタン
//      新規スレッド作成、
//      [1] active_file_manager (現在処理を行っているファイルのリスト) を生成
//      debouncer (フォルダ監視) を生成  ※ [1]＋Senderをクロージャ内へmove
//      debouncer.watch() 監視をスタート
//          ・イベントが発生
//          ・[1]に登録
//          ・新規スレッド内で処理 + 結果/エラーを送信
// 
//      引数 Sender
//      戻り値 debouncer (もしくは自作構造体やクロージャ)
//          debouncer.unwatch(Path::new("."))?; で停止させるため
//          停止時、active_file_managerもドロップされる


// 停止ボタン
//      フォルダ監視を停止
//      以下の2つのインスタンスはTauriのStateに登録
//      debouncer.unwatch();
//      ActiveFileManager::join_tasks()     非同期タスクの終了を待つ

/*
    FileManagerはmoveしてしまっている → join_tasks()を指示できない

    フィールドで2つを持つ？
        debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
        active_file_manager: Arc<Mutex< ActiveFileManager >>,
            → clone()してdebouncerへmove

    watch.rs と file_manager.rs を統合する？
        フィールドは ActiveFileManager へ debouncer を追加する形
        stopメソッドで、両方を止められる

        起動時に、TauriのStateに登録すれば、デバウンサ―の起動/停止を行いやすい
*/