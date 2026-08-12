use std::fs;
use std::path::{Path};
use std::time::{Duration, SystemTime};
use tokio::time::sleep;
use crate::models::notify::WaitResult;


/// ファイルをチェックする間隔
const CHECK_INTERVAL: Duration = Duration::from_secs(3);

/// ファイル異常回数の上限 (待機ループを抜けるまでの回数)
const FILE_ERROR_COUNT_MAX: u32 = 5;


/// ファイルの書込終了まで待機する
/// 
/// # 引数
/// * `path` - デバウンサーが検出したファイルパス
/// 
/// # 戻り値
/// 待機処理の結果値
/// 
pub async fn wait_for_file_writing(path: &Path) -> WaitResult {
    
    // メタデータの記録変数
    let mut last_size: u64 = 0;
    let mut last_modified = SystemTime::UNIX_EPOCH;

    // ファイル異常時にループを抜けるためのカウンタ
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
            Err(error) => {
                missing_count += 1;
                if missing_count >= FILE_ERROR_COUNT_MAX {
                    eprintln!("{error}");
                    return WaitResult::Missing(path.to_path_buf());
                }
            }
        }
        
        // 待機 (ファイルチェックの間隔)
        sleep(CHECK_INTERVAL).await;
    }
}

//=============================================================================
// テスト
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs::{File, OpenOptions}, path::PathBuf};
    use tokio::task::JoinSet;

    #[tokio::test]
    async fn test_wait_for_file_writing() {
        use WaitResult::*;

        // OSの安全な場所に、テスト専用一時フォルダを作成
        let tmp_dir = tempfile::tempdir().unwrap();
        let tmp_dir = tmp_dir.path();

        //-------------------------------------------------
        // テストの実行関数   
        //-------------------------------------------------

        // 全タスクの終了を待機するため
        let mut join_set = JoinSet::new();
        let join_set_ref = &mut join_set;

        // 「テスト実行・検証」処理
        let mut test = move |path: PathBuf, expected: WaitResult, test_name: &'static str| {

            // 新規タスク内で実行
            join_set_ref.spawn(async move {

                // 少なくとも3秒は待機が発生
                let result = wait_for_file_writing(&path).await;
                assert_eq!(result, expected, "{test_name}");
                // println!("{test_name}\n{:?}\n{:?}\n", result, expected);
            });
        };

        //-------------------------------------------------
        // テストの定義
        //-------------------------------------------------

        // 待機成功テスト
        let path = tmp_dir.join("test.txt");
        File::create(&path).unwrap();
        test(path.clone(), Success, "ファイル正常時の待機テスト");

        // 待機中にファイルが消失するテスト
        // ファイルを生成しないことで、消失を再現
        let path = tmp_dir.join("rewrite.txt");
        test(path.clone(), Missing(path), "ファイル消失時の待機テスト");

        // ロック中ファイルのテスト
        // Windows以外の場合、fs2 などのファイルロック用ライブラリを導入する必要がある
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;

            let path = tmp_dir.join("rock.txt");
            File::create(&path).unwrap();

            // ファイルを開いたままにする
            let locker = OpenOptions::new()
                .write(true)
                .share_mode(0)  // 0でWindowsの強力なファイルロックが発動
                .open(&path)
                .unwrap();

            test(path.clone(), Locked(path), "ファイルロック時の待機テスト");

            // ロック時のチェックループを抜けるまで待機
            let duration = CHECK_INTERVAL * (FILE_ERROR_COUNT_MAX + 1);
            sleep(duration).await;

            // ファイルのロックを解除
            drop(locker);
        }

        // 全ての処理が終わるまで待機
        join_set.join_all().await;
    }
}