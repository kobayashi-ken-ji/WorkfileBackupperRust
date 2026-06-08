use std::{time::Duration};
use tokio::time::sleep;
use tokio::sync::mpsc::{self, Sender};  // Tokio版を使用すること
use crate::models::notify::{Notifier, TimerInfo};


/// 「指定時間 経過した時」に通知する
/// 
/// ファイル未保存時間の通知用
/// スレッドを生成し、経過時間を計測する。
/// スレッドは Sender がドロップしたときに終了する。
/// 
/// 指定時間ぶん経過時に、UIへ経過時間を送信する。
/// Senderで送信を行うと、経過時間を0にリセットする。
/// リセットされるまで経過時間は累積する。
/// 
/// # 引数
/// * `timeout_mins` - 通知するまでの時間 (単位：本番は分、テストは秒)
/// * `notifier`     - UIへの通知機
/// 
/// # 戻り値
///     経過時間をリセットするための送信機 (ファイル保存時に使用)
/// 
pub fn run_timer(timeout_mins: u64, notifier: impl Notifier) -> Sender<()> {
    
    // Tokio版チャンネルを使用
    // バッファ = 送信側処理をブロックせずに済む数
    let (tx, mut rx) = mpsc::channel::<()>(32);

    let mut timeout_count = 0;

    // デバッグとテストの時のみ、秒単位に切替え
    let is_development = cfg!(any(debug_assertions, test));
    let timeout_duration = if is_development {
        Duration::from_secs(timeout_mins)
    } else {
        Duration::from_mins(timeout_mins)
    };

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
                    notifier.notify( &TimerInfo::Elapsed { minutes } );
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

//=============================================================================
// テスト
//=============================================================================

#[cfg(test)]
mod tests {
    use crate::{models::notify::{MockNotifier, ToNotify}, utilities::lock_mutex};

use super::*;

    #[test]
    fn test_run_timer() {
        
        // テスト時はTauri側ランタイムが無いため、自動で生成してくれる
        tauri::async_runtime::block_on(async {

            // テスト用の通知機を生成
            let notifier = MockNotifier::new();
            let log = notifier.log.clone();

            {
                // 3秒間ファイルが保存されなければ通知される
                let tx = run_timer(3, notifier);

                // 7秒後にファイル上書き通知
                sleep(Duration::from_secs(7)).await;
                tx.send(()).await.unwrap();

                // 4秒後に送信機が破棄される
                sleep(Duration::from_secs(4)).await;
            }
            
            // メソッド内部で notify() される値
            let expectations = [
                TimerInfo::Elapsed { minutes: 3 },
                TimerInfo::Elapsed { minutes: 6 },
                TimerInfo::Elapsed { minutes: 3 },
            ];

            // ログ値と期待値を比較
            let log = lock_mutex(&log);
            for i in 0..expectations.len() {
                assert_eq!(log[i], expectations[i].to_payload());
            }
        });
    }
}
