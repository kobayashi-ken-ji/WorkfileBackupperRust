#![allow(dead_code)]            // 未使用関数・構造体
#![allow(unused_variables)]     // 未使用変数
#![allow(unused_imports)]       // 未使用インポート

pub mod commands;
pub mod services;
pub mod models;

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

use services::watch::{NotifyMessage};

/// TauriのStateに登録するための構造体 (送信機をラップ)
pub struct MessageSender {
    pub tx: Sender<NotifyMessage>
}

//=============================================================================
// オーケストレーター（構成管理者）
// ※ Tomcat本体のような役割
//=============================================================================

// デスクトップアプリビルド : main()からこの関数を呼び出す
// モバイルアプリビルド : 静的ライブラリとして読み込まれるため、ここを呼出すように設定する
#[cfg_attr(mobile, tauri::mobile_entory_point)]
pub fn run() {
    tauri::Builder::default()

        // プラグインの初期化
        .plugin(tauri_plugin_notification::init())

        .setup(|app| {

            // 右クリックメニューの作成
            let toggle_visible = MenuItem::with_id(app, "toggle", "設定画面を開く", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "終了", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&toggle_visible, &quit])?;

            // トレイアイコンの構築
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {

                    // 「設定画面を開く」→ メインウィンドウを表示
                    "toggle" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }

                    // 「終了」 → アプリを完全終了
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            // 起動時はウィンドウを非表示
            if let Some(window) = app.get_webview_window("main") {
                // let _ = window.hide();
            }

            //-------------------------------------------------------

            // Tauri側のTokioランタイムのハンドルを取得
            // tokio::runtime::Handleに変換するには、inner()が必要
            let tokio_handle = tauri::async_runtime::handle();
            
            // 送信機/受信機を生成
            let (tx, rx) = channel::<NotifyMessage>();

            // 送信機をTauriのStateに登録
            app.manage(MessageSender { tx });
            app.manage(services::watch_manager::WatchManager::new(tokio_handle.inner()));

            // バックアップロジックをスレッドで起動
            // Tauri側のTokioランタイムを使用する
            // let tx_clone = tx.clone();
            // tauri::async_runtime::spawn(async move {
            //     loop {
            //         // tokio::~でTauri側のランタイムを使用できる
            //         tokio::time::sleep(Duration::from_secs(5)).await;
            //         let _ = tx_clone.send(LogMessage {
            //             status: "success".to_string(),
            //             message: "バックアップしました".to_string(),
            //         });
            //     }
            // });

            // Tauri用「受信・画面転送」専用スレッドを起動
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {

                // Receiverにメッセージが届くのを待ち受ける
                while let Ok(notify_message) = rx.recv() {

                    let log = notify_message.to_ui_message();
                    println!("{log}");

                    // TauriのイベントシステムでJSへ送信
                    // 第一引数: イベント名(自由)
                    // 第二引数: 送信するデータ
                    let _ = app_handle.emit("backup-event", log);
                }
            });

            Ok(())
        })

        // 「×」ボタンが押されたとき
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();    // 終了を防止
                let _ = window.hide();    // ウィンドウを非表示
            }
        })

        .invoke_handler(tauri::generate_handler![commands::start_backup])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

//=============================================================================

// UIへのデータ転送用構造体
#[derive(Clone, serde::Serialize)]  // JSONとして渡すため、Serializeを付与
#[serde(rename_all = "camelCase")]  // シリアライズ時、JSに合わせてキャメルケース化
struct LogMessage {
    status: String,
    message: String,
}
