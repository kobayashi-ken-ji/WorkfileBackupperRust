#![allow(dead_code)]            // 未使用関数・構造体
#![allow(unused_variables)]     // 未使用変数
#![allow(unused_imports)]       // 未使用インポート

// Windows環境のリリースビルドの時はコンソールを表示しない (GUIアプリ属性にする)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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
fn start_backup(app: tauri::AppHandle, path: String, extension: String) -> Result<String, String> {

    // アプリの処理を記述

    // println!("開始します");

    // デスクトップ通知を送信
    // ※ インストーラを使用しない場合、アプリ名がPowerShellになる
    app.notification()
        .builder()
        .title("バックアップを開始しました。")
        .body(&path)
        .show()
        .unwrap();

    // 画面へのレスポンス
    Ok(format!("監視を開始しました: \n{path}"))
}

//=============================================================================

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
            
            // 送信機/受信機を生成
            let (tx, rx) = channel::<LogMessage>();

            // バックアップロジックをスレッドで起動
            let tx_clone = tx.clone();
            thread::spawn(move || {
                loop {
                    thread::sleep(Duration::from_secs(5));
                    let _ = tx_clone.send(LogMessage {
                        status: "success".to_string(),
                        message: "バックアップしました".to_string(),
                    });
                }
            });

            // Tauri用「受信・画面転送」専用スレッドを起動
            let app_handle = app.handle().clone();
            thread::spawn(move || {

                // Receiverにメッセージが届くのを待ち受ける
                while let Ok(log) = rx.recv() {

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

        .invoke_handler(tauri::generate_handler![start_backup])
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
