// Atomicを使ったスレッドセーフな変数の例

fn send(self, app_handle: tauri::AppHandle) {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    // 静的変数
    // ※ 平行してテストを実行する時に問題になるため、今回は不採用
    static IS_DESKTOP_NOTIFY: AtomicBool = AtomicBool::new(true);

    // 略
            
    // Releaseで、レジスタ上のデータをメモリに反映する
    IS_DESKTOP_NOTIFY.store(is_notify, Ordering::Release);
    
    // 略

    // デスクトップ通知を行うか判定
    // Acquireで、レジスタではなくメモリから取得する
    if  ! IS_DESKTOP_NOTIFY.load(Ordering::Acquire) {
        return;
    }

    // 略
}