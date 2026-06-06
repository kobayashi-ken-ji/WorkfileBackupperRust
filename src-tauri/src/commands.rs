use tauri::{AppHandle, Manager, State, Window};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::models::config::Config;
use crate::models::notify::Notify;
use crate::models::state::ConfigState;
use crate::services::{watch::Watcher};

//=============================================================================

/// 設定ファイルを読込み、フロントへ送る
#[tauri::command]
pub fn get_config(
    app: AppHandle, window: Window, config_state: State<'_, ConfigState>) -> Config {
        
    // 設定ファイルを読込 or デフォルト値
    let config = Config::load(&app).unwrap_or_else(|error| {
        eprintln!("ファイルの読込に失敗: {error}");
        Config::default()
    });

    // 「アプリ起動時にこの画面を開く」設定を反映
    // ウィンドウ生成時は非表示状態
    if config.is_shown {
        let _ = window.show();
        let _ = window.set_focus();
        // window.hide().eprint("ウィンドウの非表示に失敗");
    }

    // Stateに設定を上書き
    config_state.write(config.clone());

    config
}

//=============================================================================
// 画面から呼び出される関数
//=============================================================================

// 引数に tauri::AppHandle を追加すると、自動で渡してくれる

/// 「開始」ボタンの処理
/// 戻り値 - Ok:開始した, Err:開始できなかった
#[tauri::command]
pub async fn start_watching(
    config_state: State<'_, ConfigState>, watcher: State<'_, Watcher>,
    app: AppHandle, mut config: Config) -> Result<(), ()> {


    // 既に開始済みの場合は停止する
    watcher.stop().await;

    // 設定ファイルを保存
    if let Err(error) = config.save(&app) {
        eprintln!("設定ファイルの保存に失敗: {}", error);

        // ※通知処理を追記する
    }

    let is_notify = config.is_notify;

    // 設定のバリデーションチェック
    config = match watcher.validate_config(config) {
        Ok(config) => config,
        Err(error) => {

            // UI/ログ用の受信機へ送信
            error.send(&app, is_notify);
            
            let message = format!("開始できませんでした\n{}", error.to_dto().body);

            let main_window = app.get_webview_window("main")
                .expect("メインウィンドウの取得に失敗");

            // モーダルダイアログを表示
            app.dialog()
                .message(message)
                // .title("サーバーエラー")
                .kind(MessageDialogKind::Info)
                .buttons(MessageDialogButtons::Ok)
                .parent(&main_window)   // メインウィンドウをブロック
                .blocking_show();

            return Err(());
        }
    };

    // Stateに設定値を上書き
    config_state.write(config.clone());

    // 開始処理を呼出し
    let result = watcher.start(app, config);
    result
}


/// 「終了」ボタンの処理
#[tauri::command]
pub async fn stop_watching(app: tauri::AppHandle, config_state: State<'_, ConfigState>,
    watcher: State<'_, Watcher>) -> Result<(), ()> {

    // 終了処理を呼出し
    let result = watcher.stop().await;

    // Stateから設定値を取得
    let is_desktop_notify = config_state.load().is_notify;

    // 結果を受信機へ送信
    result.send(&app, is_desktop_notify);

    Ok(())
}
