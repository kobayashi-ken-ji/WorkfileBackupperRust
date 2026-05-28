use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::thread;
use std::time::Duration;
use tauri::webview::cookie::time::format_description::well_known::iso8601::Config;
use tauri_plugin_notification::NotificationExt;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager,
    Emitter,
};
use tokio::sync::watch;

use crate::MessageSender;
use crate::models::message::{AppMessage, NotifyMessage};
use crate::models::message::WaitResult::Success;
use crate::services::{self, watch::Watcher};

//=============================================================================

// 画面から呼び出される関数
// 引数に tauri::AppHandle を追加すると、自動で渡してくれる
#[tauri::command]
pub async fn start_backup(
    sender:tauri::State<'_, MessageSender>, watcher: tauri::State<'_, Watcher>,
    app: tauri::AppHandle, path: String, extension: String) -> Result<(), ()> {

    use crate::models::config::Config;

    let config = Config {
        source_path: PathBuf::from(r"D:\一時作業ファイル"),
        destination_path: PathBuf::from(r"E:\old【一時作業】"),
        is_shown: true,
        is_notify: true,
        extensions: [
            "psd",
            "sai2",
            "txt",
            "tmp",  // ファイル消失テスト
            "PpP",  // 大文字小文字テスト
        ].iter().map(|str| str.to_string()).collect()
    };

    // コンソールへ設定を表示
    println!("{:#?}", config);

    // 既に開始済みの場合は停止する
    watcher.stop(Path::new("")).await;

    // アプリの処理を記述
    let tx = sender.tx.clone();
    let watch_result = watcher.start(&config, tx);

    // Err かどうかもここで返したい

    let tx = sender.tx.clone();
    let _ = tx.send(NotifyMessage::Start(watch_result));

    // let message = watch_result.to_ui_message();


    // let tx = state.tx.clone();
    // tauri::async_runtime::spawn(async move {
    //     services::watch::run(tx).unwrap();
    // });

    // デスクトップ通知を送信
    // ※ インストーラを使用しない場合、アプリ名がPowerShellになる
    // app.notification()
    //     .builder()
    //     .title(&path)
    //     .body(&path)
    //     .show()
    //     .unwrap_or_else(|e| println!("デスクトップ通知に失敗: {:?}", e));

    // 画面へのレスポンス
    // Ok(String::from("使わない"))
    Ok(())
}


#[tauri::command]
pub async fn stop_watch(
    sender:tauri::State<'_, MessageSender>, manager: tauri::State<'_, Watcher>,
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