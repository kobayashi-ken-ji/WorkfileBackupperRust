//! トレイアイコン、バージョンウィンドウを定義

use tauri::{AppHandle, Manager, Emitter, WebviewUrl, WebviewWindowBuilder};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use crate::utilities::ResutlErrPrint;

//=============================================================================
// トレイアイコン
//=============================================================================

/// トレイアイコンを生成する
pub fn init_tray(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {

    // 右クリックメニューの作成
    let version = MenuItem::with_id(app, "version", "バージョン情報", true, None::<&str>)?;
    let show  = MenuItem::with_id(app, "show", "設定画面を開く", true, None::<&str>)?;
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
}

//=============================================================================
// バージョンウィンドウ
//=============================================================================

/// バージョン情報ウィンドウを表示する
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
