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
use crate::models::config::Config;
use crate::models::message::{Notify, NotifyDTO, NotifyLevel, StopResult};
use crate::services::{self, watch::Watcher};

//=============================================================================

/// 設定ファイルを読込み、フロントへ送る
#[tauri::command]
pub fn get_config(app: tauri::AppHandle) -> Config {
    Config::load(&app)
}

//=============================================================================
// 画面から呼び出される関数
//=============================================================================

// 引数に tauri::AppHandle を追加すると、自動で渡してくれる

/// 「開始」ボタンの処理
#[tauri::command]
pub async fn start_watching(
    sender:tauri::State<'_, MessageSender>, watcher: tauri::State<'_, Watcher>,
    app: tauri::AppHandle, config: Config) -> Result<(), ()> {

    // let config = Config {
    //     source_path: PathBuf::from(r"D:\一時作業ファイル"),
    //     destination_path: PathBuf::from(r"E:\old【一時作業】"),
    //     is_shown: true,
    //     is_notify: true,
    //     extensions: [
    //         "psd",
    //         "sai2",
    //         "txt",
    //         "tmp",  // ファイル消失テスト
    //         "PpP",  // 大文字小文字テスト
    //     ].iter().map(|str| str.to_string()).collect()
    // };

    // コンソールへ設定を表示
    // println!("{:#?}", config);

    // 既に開始済みの場合は停止する
    watcher.stop().await;

    // 設定ファイルを保存
    if let Err(error) = config.save(&app) {
        println!("設定ファイルの保存に失敗: {}", error);
    }

    // 開始処理を呼出し
    let tx = sender.tx.clone();
    let result = watcher.start(&config, tx);
    let dto = result.get_dto();
    let is_error = dto.level == NotifyLevel::Error;

    // 結果を受信機へ送信
    let tx = sender.tx.clone();
    let _ = tx.send(dto);

    if is_error { Err(()) }
    else { Ok(()) }
}


/// 「終了」ボタンの処理
#[tauri::command]
pub async fn stop_watching(
    sender:tauri::State<'_, MessageSender>, watcher: tauri::State<'_, Watcher>,
    app: tauri::AppHandle) -> Result<(), ()> {

    // 終了処理を呼出し
    let result = watcher.stop().await;
    let dto = result.get_dto();

    // 結果を受信機へ送信
    let tx = sender.tx.clone();
    let _ = tx.send(dto);

    Ok(())
}
