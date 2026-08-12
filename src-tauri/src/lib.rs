// #![allow(dead_code)]            // 未使用関数・構造体
// #![allow(unused_variables)]     // 未使用変数
// #![allow(unused_imports)]       // 未使用インポート

pub mod commands;
pub mod services;
pub mod models;
pub mod window;
pub mod utilities;

//=============================================================================
// オーケストレーター（構成管理者）
//=============================================================================

// デスクトップアプリビルド : main()からこの関数を呼び出す
// モバイルアプリビルド : 静的ライブラリとして読み込まれるため、ここを呼出すように設定する
#[cfg_attr(mobile, tauri::mobile_entory_point)]
pub fn run() {

    use tauri::Manager;
    use crate::models::state::ConfigState;
    use crate::services::app_manager::AppManager;

    // Tauri側のTokioランタイムのハンドルを取得
    // tokio::runtime::Handleに変換するには、inner()が必要
    let tokio_handle = tauri::async_runtime::handle();

    tauri::Builder::default()

        // プラグインの初期化
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cmd| {

            // 多重起動防止
            // 既存のウィンドウを取得、最前面に出す
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();         // hideを解除
                let _ = window.unminimize();   // 最小化を解除
                let _ = window.set_focus();
            }
        }))

        // Tauri::Stateに登録
        .manage(ConfigState::new())
        .manage(AppManager::new(tokio_handle.inner()))

        // 引数のクロージャ内は別スレッドで実行される模様
        // JSからアクセスが必要なものは、先にmanageしておく
        .setup(window::init_tray)

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

        // JavaScriptへ関数を公開する
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::start_watching,
            commands::stop_watching,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
