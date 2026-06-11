//! JavaScript から呼び出される処理を定義

use tauri::{AppHandle, State, Window};

use crate::models::config::Config;
use crate::models::state::ConfigState;
use crate::models::notify::{AppNotifier, Notifier, MockNotifier};
use crate::services::app_manager::AppManager;
use crate::window::TrayMenuItems;


/// HTML生成直後の処理
/// 設定ファイルを読込み、JSへ返す
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


/// 「開始」ボタンの処理
/// 戻り値 - Ok:開始した, Err:開始できなかった
#[tauri::command]
pub async fn start_watching(
    config_state: State<'_, ConfigState>, app_manager: State<'_, AppManager>,
    tray_menu: State<'_, TrayMenuItems>, app: AppHandle, mut config: Config
) -> Result<(), ()> {


    // メニューの表示切替
    tray_menu.onstart_or_onstop();

    // 既に開始済みの場合は停止する
    app_manager.stop().await;

    // 設定ファイルを保存
    if let Err(error) = config.save(&app) {
        eprintln!("設定ファイルの保存に失敗: {}", error);

        // ※UIへ通知が必要な場合はここに追記
    }

    // UIへの送信機を生成
    // テスト時の不具合を避けるため、cfg で分岐処理を行う
    let notifier = {
        #[cfg(not(test))]
        { Notifier::Real(AppNotifier::new(&app, config.is_notify)) }
        #[cfg(test)]
        { Notifier::Mock(MockNotifier::new()) }
    };

    let unsaved_notifier = {
        #[cfg(not(test))]
        { Notifier::Real(AppNotifier::new(&app, config.is_notify_unsaved)) }
        #[cfg(test)]
        { Notifier::Mock(MockNotifier::new()) }
    };
    
    // 設定値をチェック
    if let Err(error) = config.validate() {

        // UIへエラーを送信
        notifier.notify(&error);

        // メニューの表示切替
        tray_menu.starterr_or_stopok();

        return Err(());
    }

    // Stateに設定値を上書き
    config_state.write(config.clone());

    // 開始処理を呼出し
    let result = app_manager.start(config, notifier, unsaved_notifier);

    // メニューの表示切替
    match result {
        Ok(())  => tray_menu.startok_or_stoperr(),
        Err(()) => tray_menu.starterr_or_stopok(),
    }

    result
}


/// 「終了」ボタンの処理
#[tauri::command]
pub async fn stop_watching(
    app: tauri::AppHandle, config_state: State<'_, ConfigState>,
    tray_menu: State<'_, TrayMenuItems>, app_manager: State<'_, AppManager>
) -> Result<(), ()> {

    // メニューの表示切替
    tray_menu.onstart_or_onstop();

    // 監視スレッドを停止
    let result = app_manager.stop().await;

    // Stateから設定値を取得
    let is_desktop_notify = config_state.load().is_notify;

    // 結果をUIへ送信
    let notifier = AppNotifier::new(&app, is_desktop_notify);
    notifier.notify(&result);

    // メニューの表示切替
    tray_menu.starterr_or_stopok();

    Ok(())
}
