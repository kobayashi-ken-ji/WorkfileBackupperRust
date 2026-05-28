use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tokio::time::sleep;

use crate::models::message::WaitResult;


/// 指定ファイルの書込終了まで待機する
pub async fn wait_file_writing(path: &Path) -> WaitResult {
    
    /// ファイルをチェックする間隔
    const CHECK_INTERVAL: Duration = Duration::from_secs(3);
    
    // メタデータの記録変数
    let mut last_size: u64 = 0;
    let mut last_modified = SystemTime::UNIX_EPOCH;

    // ファイル異常時にループを抜けるためのカウンタ
    const FILE_ERROR_COUNT_MAX: i32 = 5;
    let mut missing_count = 0;
    let mut locked_count  = 0;

    loop {
        // メタデータを取得
        match fs::metadata(&path) {

            Ok(metadata) => {
                let current_size = metadata.len();
                let current_modified = metadata.modified().unwrap_or(last_modified);

                // 書き込み終了チェック (サイズ・タイムスタンプが変更なし)
                if (current_size == last_size) && (current_modified == last_modified) {

                    // ファイルロックが解除されていれば、待機終了
                    if fs::OpenOptions::new().write(true).open(&path).is_ok() {
                        return WaitResult::Success;
                    }

                    // ロック未解除が続く場合、タイムアウトさせる
                    else {
                        locked_count += 1;
                        if locked_count >= FILE_ERROR_COUNT_MAX {
                            return WaitResult::Locked(path.to_path_buf());
                        }
                    }
                }

                // ファイルに変動がある場合は、ファイル異常数を初期化
                else { locked_count = 0; }

                // 前回情報を更新
                last_size = current_size;
                last_modified = current_modified;
            }

            // ファイルが存在しない、または読取権限がない
            // ファイル消失時(.tmpなど)が続く場合、タイムアウトさせる
            Err(_) => {
                missing_count += 1;
                if missing_count >= FILE_ERROR_COUNT_MAX {
                    return WaitResult::Missing(path.to_path_buf());
                }
            }
        }
        
        // 待機 (ファイルチェックの間隔)
        sleep(CHECK_INTERVAL).await;
    }
}
