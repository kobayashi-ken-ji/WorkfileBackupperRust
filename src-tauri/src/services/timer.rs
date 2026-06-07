use std::{time::Duration};
use tokio::time::sleep;
use tokio::sync::mpsc::{self, Sender};  // Tokio版を使用すること
use crate::models::notify::{Notify, TimerInfo};


/// 「指定時間 経過した時」に通知する
/// 
/// ファイル未保存時間の通知用
/// スレッドを作り、経過時間を計測する。
/// 
/// 指定時間ぶん経過時に、UIへ経過時間を送信する。
/// 戻り値の Sender がドロップされるまで繰り返される。
/// 
/// Senderで送信を行うと、経過時間を0にリセットする。
/// リセットされるまで経過時間は累積する。
/// 
/// # 引数
/// * `timeout_mins` - 通知するまでの時間 (単位：分)
/// * `app`          - 送信に必要なアプリハンドル
/// 
/// # 戻り値
///     経過時間をリセットするための送信機 (ファイル保存時に使用)
/// 
pub fn run_timer(timeout_mins: u64, app: tauri::AppHandle) -> Sender<()> {
    
    // Tokio版チャンネルを使用
    // バッファ = 送信側処理をブロックせずに済む数
    let (tx, mut rx) = mpsc::channel::<()>(32);

    let mut timeout_count = 0;

    // [!] テスト用に秒を使用中
    let timeout_duration = Duration::from_secs(timeout_mins);

    // 受信機ループ
    tokio::spawn(async move {
        println!("時間計測を開始");

        loop {
            // 先に届いたイベントを実行する
            tokio::select! {

                // 指定時間経過
                _ = sleep(timeout_duration) => {
                    timeout_count += 1;

                    // 経過時間を、UI用の受信機へ送る
                    let minutes = timeout_mins * timeout_count;
                    TimerInfo::Elapsed { minutes }.send(&app, true);
                }
                
                // 受信 (ファイルがバックアップされた)
                cmd = rx.recv() => {
                    match cmd {
                        Some(_) => {
                            println!("経過時間をリセット");
                            timeout_count = 0;
                        }
                        None => break,  // チャンネルが閉じたら終了
                    }
                }
            }
        }
        println!("時間計測を終了");
    });

    tx
}

// Tauriランタイムで動作させる
// rt_handle: tokio::runtime::Handle
// rt_handle.spawn(async {
//     println!("指定されたランタイム上で動いています");
// });


// async fn run() {

//     // バッファ = 送信機側処理をブロックせずに済む数
//     let (tx, mut rx) = mpsc::channel::<()>(32);

//     let timeout_secs = 5;
//     let timeout_duration = Duration::from_secs(timeout_secs);

//     let mut timeout_count = 0;

//     // 受信機ループ
//     // tauri::async_runtime::spawn
//     tokio::spawn(async move {
//         loop {
            
//             // いずれかうちの速いイベントを実行する
//             tokio::select! {

//                 // 指定時間経過した
//                 _ = sleep(timeout_duration) => {
//                     timeout_count += 1;
//                     println!("未保存期間: {}秒", timeout_secs * timeout_count);

//                     // UI通知用の受信機へ送る
//                 }
                
//                 // 受信 (ファイルがバックアップされた)
//                 cmd = rx.recv() => {
//                     match cmd {
//                         Some(_) => {
//                             println!("タイムリセット");
//                             timeout_count = 0;
//                         }
//                         None => break,  // チャンネルが閉じたら終了
//                     }
//                 }
//             }
//         }
//         println!("受信終了");
//     });

    
//     // テスト送信

//     // 12秒後にファイル上書き通知
//     sleep(Duration::from_secs(12)).await;
//     tx.send(()).await.unwrap();

//     // 7秒後に送信機が破棄される
//     sleep(Duration::from_secs(7)).await;
// }


//=============================================================================
// テスト
//=============================================================================

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn test() {
        
//         // テスト時はTauri側ランタイムが無いため、自動で生成してくれる
//         tauri::async_runtime::block_on(async {
//             run_timer(10, app);
//         });
//     }
// }
