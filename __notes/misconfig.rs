
//=============================================================================
// ユーザー入力のエラー表示
//=============================================================================

fn open_misconfig_window(app: &tauri::AppHandle) {

    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

    let answer = app.dialog()
        .message("設定が正しくありません")
        // .title("サーバーエラー")
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::Ok)
        .blocking_show();
}

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

//=============================================================================

// tauri.conf.json で記述する場合
//      ただし、常にメモリを消費する
//   {
//     "label": "version-info",
//     "title": "バージョン情報",
//     "url": "version.html",
//     "width": 400,
//     "height": 200,
//     "center": true,
//     "visible": false
//   }
