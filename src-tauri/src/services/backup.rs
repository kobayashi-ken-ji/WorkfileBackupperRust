use std::fs;
use std::path::{PathBuf, Path};
use chrono::{DateTime, Local};

//=============================================================================
// 戻り値の型
//=============================================================================

#[derive(Debug, Clone, serde::Serialize)]
pub enum BackupResult {
    Copied          (PathBuf),
    AlreadyExists   (PathBuf),
    InvalidFileName (PathBuf),
    MetadataFailed  { path: PathBuf, error: String },  // std::io::Errorを文字列化
    ModifiedFailed  { path: PathBuf, error: String },
    CopyFailed      { path: PathBuf, error: String },
}

impl BackupResult {

    /// UI表示用メッセージを取得
    pub fn to_ui_message(&self) -> String {
        use BackupResult::*;
        match self {
            Copied          (path)     => format!("バックアップ: {}", path.display()),
            AlreadyExists   (path)     => format!("既にバックアップ済み: {}", path.display()),
            InvalidFileName (path)     => format!("ファイル名の取得に失敗: {}", path.display()),
            MetadataFailed  {path, ..} => format!("ファイル情報の取得に失敗: {}", path.display()),
            ModifiedFailed  {path, ..} => format!("最終更新時の取得に失敗: {}", path.display()),
            CopyFailed      {path, ..} => format!("コピーに失敗: {}", path.display()),
        }
    }
}

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
    pub fn new(destination: &str) -> Self {
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
            Err(e) => return MetadataFailed { path:path.to_path_buf(), error:e.to_string() },
        };

        // 最終更新時を取得
        let system_time = match metadata.modified() {
            Ok(time) => time,
            Err(e) => return ModifiedFailed { path:path.to_path_buf(), error:e.to_string() },
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
            Err(e) => CopyFailed { path: new_path, error: e.to_string() },
        }
    }
}

//=============================================================================
// テスト
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_result() {

        // 存在するフォルダ
        let backupper = FileBackupper::new(r"D:\backuper_test");
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
            println!("{}", result.to_ui_message());
            println!("{:?}", result);
            println!();
        }

        //-----------------------------------------

        // コピーに失敗させる
        let backupper = FileBackupper::new(r"D:\存在しないフォルダ");
        assert_eq!(backupper.is_valid(), false);

        let source = PathBuf::from(paths[0]);
        let result = backupper.backup_file(&source);
        println!("{}", result.to_ui_message());
        println!("{:?}", result);
        assert!(matches!(result, BackupResult::CopyFailed{..}));
    }
}