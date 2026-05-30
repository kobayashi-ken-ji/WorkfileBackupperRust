use std::fs;
use std::path::{PathBuf, Path};
use chrono::{DateTime, Local};
use crate::models::message::BackupResult;

//=============================================================================
// バックアップ処理
//=============================================================================

#[derive(Debug, Clone)]
pub struct FileBackupper {
    /// コピー先ディレクトリ
    destination: PathBuf,
}

impl FileBackupper {

    /// コンストラクタ
    pub fn new(destination: &Path) -> Self {
        Self {destination: PathBuf::from(destination)}
    }


    /// コピー先ディレクトリが有効かチェック
    pub fn is_valid(&self) -> bool {
        self.destination.exists() && self.destination.is_dir()
    }

 
    /// 指定ファイルをバックアップ
    pub fn backup_file(&self, path: &Path) -> BackupResult {
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

        // 更新時 → ローカルタイムゾーン → YYYYMMDD_HHMMSS形式 へ変換
        let local_time: DateTime<Local> = DateTime::from(system_time);
        let time_stamp = local_time.format("%Y%m%d_%H%M%S").to_string();

        // ファイル名を取得
        let Some(file_stem) = path.file_stem() else {
            return InvalidFileName(path.to_path_buf());
        };

        // バックアップ用ファイル名を生成 (拡張子なし)
        // 元ファイル名[YYYYMMDD_HHMMSS]
        let new_file_name =
            format!("{}[{}]", file_stem.to_string_lossy(), time_stamp);

        // パスを生成 (ディレクトリ/新ファイル名)
        let mut new_path = self.destination.clone();
        new_path.push(new_file_name);

        // 拡張子がある場合は付与
        if let Some(extention) = path.extension() {
            new_path.set_extension(extention);
        }

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
}

//=============================================================================
// テスト
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::message::{Notify};

    #[test]
    fn one_result() {

        // 存在するフォルダ
        let backupper = FileBackupper::new(Path::new(r"D:\backuper_test"));
        assert_eq!(backupper.is_valid(), true);

        // テストケース
        let paths = [
            r"D:\backuper_test\存在する.txt",
            r"D:\backuper_test\拡張子なし",      // コピー可
            r"D:\backuper_test\存在しない.txt",  // エラー
        ];

        for path in paths {
            let source = PathBuf::from(path);
            let result = backupper.backup_file(&source);
            let dto = result.get_dto();
            println!("{}: {}", dto.title, dto.body);
            println!("{:?}", result);
            println!();
        }

        //-----------------------------------------

        // コピーに失敗させる
        let backupper = FileBackupper::new(Path::new(r"D:\存在しないフォルダ"));
        assert_eq!(backupper.is_valid(), false);

        let source = PathBuf::from(paths[0]);
        let result = backupper.backup_file(&source);
        let dto = result.get_dto();
        println!("{}: {}", dto.title, dto.body);
        println!("{:?}", result);
        assert!(matches!(result, BackupResult::CopyFailed{..}));
    }
}