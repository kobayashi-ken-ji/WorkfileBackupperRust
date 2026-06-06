use std::fs;
use std::path::{PathBuf, Path};
use chrono::{DateTime, Local};
use crate::models::notify::BackupResult;

//=============================================================================
// バックアップ処理
//=============================================================================

/// 指定ファイルをバックアップ
/// 引数 - コピー先フォルダ, コピーするファイル
pub fn backup_file(destination: &Path, path: &Path) -> BackupResult {
    use BackupResult::*;

    // メタデータの取得
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            eprintln!("{error}");
            return MetadataFailed(path.to_path_buf());
        }
    };

    // 最終更新時を取得
    let system_time = match metadata.modified() {
        Ok(time) => time,
        Err(error) => {
            eprintln!("{error}");
            return ModifiedFailed(path.to_path_buf());
        }
    };

    // 更新時 → ローカルタイムゾーン → [YYYYMMDD_HHMMSS]形式 へ変換
    let local_time: DateTime<Local> = DateTime::from(system_time);
    let time_stamp = local_time.format("[%Y%m%d_%H%M%S]").to_string();

    /*
        ファイル名にドットが含まれる場合
            1.00 というファイル名に set_extension() すると
            00 を上書きしてしまうので、push()を使用する

        Windowsの拡張パスの場合
            PathBuf::push() すると拡張子のドットをフォルダ階層と認識してしまう
            そのため、OsStringでファイル名を生成する
     */

    // ファイル名を取得
    let Some(file_stem) = path.file_stem() else {
        return InvalidFileName(path.to_path_buf());
    };

    // ファイル名 + [YYYYMMDD_HHMMSS]
    let mut new_file_name = file_stem.to_os_string();
    new_file_name.push(time_stamp);

    // 拡張子がある場合は付与
    if let Some(extention) = path.extension() {
        new_file_name.push(".");
        new_file_name.push(extention);
    }

    // パスを生成 (ディレクトリ/新ファイル名)
    let mut new_path = PathBuf::from(destination);
    new_path.push(new_file_name);

    // 同名のファイルが既に存在するか確認
    if new_path.exists() {
        return AlreadyExists(new_path);
    }

    // バックアップ実行
    match fs::copy(path, &new_path) {
        Ok(_)  => Copied(new_path),
        Err(error) => {
            eprintln!("{error}");
            return CopyFailed(new_path);
        }
    }
}


/// 「フォルダ階層に対応したコピー先」のパスを生成
/// 無ければフォルダも生成
/// 問題発生時は destination をそのまま返す
/// 
/// 例
/// コピー元フォルダ: /sorce/
/// コピー先フォルダ: /dest/
/// コピーするファイル: /sorce/folder/file.txt   ※ folderを検出
/// 戻り値: /dest/folder/                       ※ コピー先に結合
pub fn get_destination_for_recursive(source: &Path, destination: &Path, file: &Path) -> PathBuf {

    // この時点で正規化されている
    // println!("検出したファイル: {:?}", file);

    // 親ディレクトリを抽出 (ファイル名の除去)
    let Some(parent) = file.parent() else {
        eprint!("親ディレクトリの取得に失敗: {:?}", file);
        return PathBuf::from(destination);
    };

    // 相対パスを抽出 (監視フォルダ部分の除去)
    let relative_path = match parent.strip_prefix(source) {
        Ok(p) => p,
        Err(error) => {
            eprint!("相対パス化に失敗: {error}");
            return PathBuf::from(destination);
        }
    };
    
    // 新ファイルパス (行先フォルダ + 相対パス)
    let new_path = destination.join(relative_path);

    // 親ディレクトリが無ければ作成
    if let Err(error) = fs::create_dir_all(&new_path) {
        eprintln!("親ディレクトリの作成に失敗: {error}");
        return PathBuf::from(destination);
    }

    // println!("サブフォルダ対応後のコピー先フォルダ: {:?}", new_path);
    PathBuf::from(new_path)
}


// #[derive(Debug, Clone)]
// pub struct FileBackupper {
//     /// コピー先ディレクトリ
//     destination: PathBuf,
// }

// impl FileBackupper {

//     /// コンストラクタ
//     pub fn new(destination: &Path) -> Self {
//         Self {destination: PathBuf::from(destination)}
//     }


//     /// コピー先ディレクトリが有効かチェック
//     pub fn is_valid(&self) -> bool {
//         self.destination.exists() && self.destination.is_dir()
//     }

 
//     /// 指定ファイルをバックアップ
//     pub fn backup_file(&self, path: &Path) -> BackupResult {
//         use BackupResult::*;

//         // メタデータの取得
//         let metadata = match fs::metadata(path) {
//             Ok(metadata) => metadata,
//             Err(error) => {
//                 eprintln!("{error}");
//                 return MetadataFailed(path.to_path_buf());
//             }
//         };

//         // 最終更新時を取得
//         let system_time = match metadata.modified() {
//             Ok(time) => time,
//             Err(error) => {
//                 eprintln!("{error}");
//                 return ModifiedFailed(path.to_path_buf());
//             }
//         };

//         // 更新時 → ローカルタイムゾーン → YYYYMMDD_HHMMSS形式 へ変換
//         let local_time: DateTime<Local> = DateTime::from(system_time);
//         let time_stamp = local_time.format("%Y%m%d_%H%M%S").to_string();

//         // ファイル名を取得
//         let Some(file_stem) = path.file_stem() else {
//             return InvalidFileName(path.to_path_buf());
//         };

//         // バックアップ用ファイル名を生成 (拡張子なし)
//         // 元ファイル名[YYYYMMDD_HHMMSS]
//         let new_file_name =
//             format!("{}[{}]", file_stem.to_string_lossy(), time_stamp);

//         // パスを生成 (ディレクトリ/新ファイル名)
//         let mut new_path = self.destination.clone();
//         new_path.push(new_file_name);

//         // 拡張子がある場合は付与
//         if let Some(extention) = path.extension() {
//             new_path.set_extension(extention);
//         }

//         // 同名のファイルが既に存在するか確認
//         if new_path.exists() {
//             return AlreadyExists(new_path);
//         }

//         // バックアップ実行
//         match fs::copy(path, &new_path) {
//             Ok(_)  => Copied(new_path),
//             Err(error) => {
//                 eprintln!("{error}");
//                 return CopyFailed(new_path);
//             }
//         }
//     }
// }

// //=============================================================================
// // テスト
// //=============================================================================

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::models::notify::{Notify};

//     #[test]
//     fn one_result() {

//         // 存在するフォルダ
//         let backupper = FileBackupper::new(Path::new(r"D:\backuper_test"));
//         assert_eq!(backupper.is_valid(), true);

//         // テストケース
//         let paths = [
//             r"D:\backuper_test\存在する.txt",
//             r"D:\backuper_test\拡張子なし",      // コピー可
//             r"D:\backuper_test\存在しない.txt",  // エラー
//         ];

//         for path in paths {
//             let source = PathBuf::from(path);
//             let result = backupper.backup_file(&source);
//             let dto = result.to_dto();
//             println!("{}: {}", dto.title, dto.body);
//             println!("{:?}", result);
//             println!();
//         }

//         //-----------------------------------------

//         // コピーに失敗させる
//         let backupper = FileBackupper::new(Path::new(r"D:\存在しないフォルダ"));
//         assert_eq!(backupper.is_valid(), false);

//         let source = PathBuf::from(paths[0]);
//         let result = backupper.backup_file(&source);
//         let dto = result.to_dto();
//         println!("{}: {}", dto.title, dto.body);
//         println!("{:?}", result);
//         assert!(matches!(result, BackupResult::CopyFailed{..}));
//     }
// }