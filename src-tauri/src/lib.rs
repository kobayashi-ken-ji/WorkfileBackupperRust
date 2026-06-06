// #![allow(dead_code)]            // 未使用関数・構造体
// #![allow(unused_variables)]     // 未使用変数
// #![allow(unused_imports)]       // 未使用インポート

pub mod commands;
pub mod services;
pub mod models;

use tauri::{AppHandle, Manager, Emitter, WebviewUrl, WebviewWindowBuilder};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;

use crate::models::state::ConfigState;
use crate::models::eprint::ResutlErrPrint;
use crate::services::watch::Watcher;

//=============================================================================
// オーケストレーター（構成管理者）
// ※ Tomcat本体のような役割
//=============================================================================

// デスクトップアプリビルド : main()からこの関数を呼び出す
// モバイルアプリビルド : 静的ライブラリとして読み込まれるため、ここを呼出すように設定する
#[cfg_attr(mobile, tauri::mobile_entory_point)]
pub fn run() {

    // Tauri側のTokioランタイムのハンドルを取得
    // tokio::runtime::Handleに変換するには、inner()が必要
    let tokio_handle = tauri::async_runtime::handle();

    tauri::Builder::default()

        // プラグインの初期化
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())

        // 多重起動防止
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cmd| {

            // 既存のウィンドウを取得、最前面に出す
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();         // hideを解除
                let _ = window.unminimize();   // 最小化を解除
                let _ = window.set_focus();
            }
        }))

        // Tauri::Stateに登録
        .manage(ConfigState::new())
        .manage(Watcher::new(tokio_handle.inner()))

        // 引数のクロージャ内は別スレッドで実行される模様
        // JSからアクセスが必要なものは、先にmanageしておく
        .setup(|app| {

            // 右クリックメニューの作成
            let version = MenuItem::with_id(app, "version", "バージョン情報", true, None::<&str>)?;
            let show  = MenuItem::with_id(app, "show", "設定 / ログ", true, None::<&str>)?;
            let quit  = MenuItem::with_id(app, "quit", "終了", true, None::<&str>)?;
            let start = MenuItem::with_id(app, "start", "開始", true, None::<&str>)?;
            let stop  = MenuItem::with_id(app, "stop", "停止", true, None::<&str>)?;
            let menu  = Menu::with_items(app, &[&version, &show, &start, &stop, &quit])?;

            // トレイアイコンの構築
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().expect("アイコン取得に失敗").clone())
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {

                    "version" => open_vesion_window(app),

                    // 「設定画面を開く」→ メインウィンドウを表示
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }

                    // フォルダ監視の 開始/終了
                    "start" => { app.emit("start", ()).eprint("JSへの開始通知に失敗"); }
                    "stop"  => { app.emit("stop", ()) .eprint("JSへの停止通知に失敗"); }

                    // 「終了」 → アプリを完全終了
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })

        .on_window_event(|window, event| {

            // 「×」ボタンが押されたとき
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {

                match window.label() {
                    "main" => {
                        api.prevent_close();    // ウィンドウが閉じられることを防止
                        let _ = window.hide();  // ウィンドウを非表示
                    }
                    _ => {}
                }
            }
        })

        // JavaScriptへ関数を登録する
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::start_watching,
            commands::stop_watching,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

//=============================================================================
// バージョンウィンドウ
//=============================================================================

pub fn open_vesion_window(app: &AppHandle) {

    const WINDOW_LABEL: &str = "version-info";

    // 重複表示防止
    if let Some(existing_window) = app.get_webview_window(WINDOW_LABEL) {
        let _ = existing_window.set_focus();
        return;
    }

    // ウィンドウの構築
    let url = WebviewUrl::App("version.html".into());
    WebviewWindowBuilder::new(app, WINDOW_LABEL, url)
        .title("バージョン情報")
        .inner_size(400.0, 200.0)
        .resizable(false)
        .center()
        // .always_on_top(true)
        .build()
        .eprint("バージョン表示ウィンドウのビルドに失敗");
}
