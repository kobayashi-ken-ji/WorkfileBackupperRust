

pub fn send(app_handle: tauri::AppHandle) {

    // デスクトップ通知を行うか
    let mut is_desktop_notify = false;

    // Receiverにメッセージが届くのを待ち受ける
    while let Ok(package) = rx.recv() {

        // 通知 / 設定変更 の判別
        let dto = match package {
            NotifyPackage::Message(dto) => dto,
            NotifyPackage::Config { is_notify } => {
                is_desktop_notify = is_notify;
                continue;
            }
        };

        let message = format!("{}: {}", dto.title, dto.body);

        // デバッグ時のみコンソールへ表示
        if cfg!(debug_assertions) {
            println!("{message}");
        }

        // Debug → コンソール以外には表示しない
        if dto.level == NotifyLevel::Debug { continue; }

        // TauriのイベントシステムでJSへ送信
        // 第一引数: イベント名(自由)
        // 第二引数: 送信するデータ
        app_handle.emit("log-event", &dto).eprint("JSへのログ通知に失敗");

        // デスクトップ通知を行うか判定
        if  !is_desktop_notify ||
            dto.level == NotifyLevel::Silent ||
            dto.level == NotifyLevel::ErrorSilent {
            continue;
        }

        // メインスレッドから通知しないと、Windowsは1度保留にしてしまう
        let app_handle_clone = app_handle.clone();
        app_handle.run_on_main_thread(move || {

            // デスクトップ通知を送信
            // ※ インストーラを使用しない場合、アプリ名がPowerShellになる
            app_handle_clone.notification()
                .builder()
                .title(dto.title)
                .body(dto.body)
                .show()
                .eprint("デスクトップ通知に失敗");

        }).eprint("メインスレッドへの委託に失敗");
    }
});