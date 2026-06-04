use tauri::{AppHandle, Manager, State, WebviewWindowBuilder, Window, window};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};


use crate::MessageSender;
use crate::models::config::Config;
use crate::models::eprint::ResutlErrPrint;
use crate::models::notify::{Notify, NotifyPackage};
use crate::services::{watch::Watcher};

const SEND_ERROR: &str = "メッセージ受信機がドロップされています";

//=============================================================================

/// 設定ファイルを読込み、フロントへ送る
#[tauri::command]
pub fn get_config(
    app: AppHandle, window: Window, sender:State<'_, MessageSender>) -> Config {
        
    // 設定ファイルを読込 or デフォルト値
    let config = Config::load(&app).unwrap_or_else(|error| {
        eprintln!("ファイルの読込に失敗: {error}");
        Config::default()
    });

    // 「アプリ起動時にこの画面を開く」設定を反映
    if !config.is_shown {
        let _ = window.hide();
    }

    // 「デスクトップ通知をする」設定を送信
    let tx = sender.tx.clone();
    let package = NotifyPackage::Config { is_notify: config.is_notify };
    tx.send(package).eprint(SEND_ERROR);

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
    sender: State<'_, MessageSender>, watcher: State<'_, Watcher>,
    app: AppHandle, mut config: Config) -> Result<(), ()> {
    
    // 「デスクトップ通知をする」設定を送信
    let tx = sender.tx.clone();
    let package = NotifyPackage::Config { is_notify: config.is_notify };
    tx.send(package).eprint(SEND_ERROR);

    // 既に開始済みの場合は停止する
    watcher.stop().await;

    // 設定ファイルを保存
    if let Err(error) = config.save(&app) {
        eprintln!("設定ファイルの保存に失敗: {}", error);

        // ※通知処理を追記する
    }

    // 設定のバリデーションチェック
    config = match watcher.validate_config(config) {
        Ok(config) => config,
        Err(error) => {

            // UI/ログ用の受信機へ送信
            error.send(&tx);
            
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

    // 開始処理を呼出し
    let result = watcher.start(&config, tx);

    result
}


/// 「終了」ボタンの処理
#[tauri::command]
pub async fn stop_watching(sender: State<'_, MessageSender>,
    watcher: State<'_, Watcher>) -> Result<(), ()> {

    // 終了処理を呼出し
    let result = watcher.stop().await;

    // 結果を受信機へ送信
    result.send(&sender.tx);

    Ok(())
}

//=============================================================================
// ユーザー入力のエラー表示
//=============================================================================
// fn open_misconfig_window(app: &tauri::AppHandle) {

//     use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

//     let answer = app.dialog()
//         .message("設定が正しくありません")
//         // .title("サーバーエラー")
//         .kind(MessageDialogKind::Info)
//         .buttons(MessageDialogButtons::Ok)
//         .blocking_show();


//     // let main_window = app.get_webview_window("main").expect("メインウィンドウの取得に失敗");

//     // let child = WebviewWindowBuilder::new(
//     //     app,
//     //     "modal-window",
//     //     tauri::WebviewUrl::App("misconfig.html".into())
//     // );

//     // let child = child.parent(&main_window).unwrap();

//     // let window = child
//     //     .title("開始できませんでした")
//     //     .inner_size(400.0, 300.0)
//     //     // .always_on_top(true)
//     //     .build();

//     // let Ok(window) = window else {
//     //     eprintln!("設定エラーウィンドウの表示に失敗");
//     //     return;
//     // };

//     // // 親ウィンドウをセット
//     // if let Some(parent) = app.get_webview_window("main") {
//     //     window.set_parent
//     // }
// }

// fn open_misconfig_window(app: &tauri::AppHandle) {

//     let main_window = app.get_webview_window("main").expect("メインウィンドウの取得に失敗");

//     let child = WebviewWindowBuilder::new(
//         app,
//         "modal-window",
//         tauri::WebviewUrl::App("misconfig.html".into())
//     );

//     let child = child.parent(&main_window).unwrap();

//     let window = child
//         .title("開始できませんでした")
//         .inner_size(400.0, 300.0)
//         // .always_on_top(true)
//         .build();

//     // let Ok(window) = window else {
//     //     eprintln!("設定エラーウィンドウの表示に失敗");
//     //     return;
//     // };

//     // // 親ウィンドウをセット
//     // if let Some(parent) = app.get_webview_window("main") {
//     //     window.set_parent
//     // }
// }