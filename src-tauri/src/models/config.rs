use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::Manager;

use crate::models::notify::ConfigError;


/// ユーザー設定
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]  // JSONとして渡すため、Serializeを付与
#[serde(rename_all = "camelCase")]  // シリアライズ時、JSに合わせてキャメルケース化
pub struct Config {

    // バックアップ設定
    pub source_path: PathBuf,       // バックアップ元フォルダ
    pub destination_path: PathBuf,  // バックアップ先フォルダ
    pub recursive: bool,            // サブフォルダを含める
    pub all_files_enabled: bool,    // 全てのファイルをバックアップする
    pub extensions: Vec<String>,    // バックアップするファイルの種類 (拡張子)

    // デスクトップ通知の設定
    pub is_notify: bool,            // デスクトップ通知をする
    pub is_notify_unsaved: bool,    // ファイル未保存時間を通知
    pub notify_interval: u64,       // 未保存時間の通知間隔 (分)

    // アプリ起動時の設定
    pub is_shown: bool,             // アプリ起動時にウィンドウを表示する
    pub auto_start: bool,           // アプリ起動時に自動的に開始する
}


// デフォルト値を定義
impl Default for Config {
    fn default() -> Self {
        Self {
            source_path: PathBuf::from(r"D:\一時作業ファイル"),
            destination_path: PathBuf::from(r"E:\old【一時作業】"),
            recursive: true,
            all_files_enabled: false,
            extensions: vec![
                String::from("txt"),
                String::from("psd"),
                String::from("sai2"),
                String::from("jpg"),
                String::from("tmp"),    // ファイル消失テスト
                String::from("PpP"),    // 大文字小文字テスト
            ],
            is_notify: true,
            is_notify_unsaved: false,
            notify_interval: 30,
            is_shown: true,
            auto_start: false,
        }
    }
}


impl Config {

    /// 設定ファイルの保存先パスを取得
    /// ※ Windows環境での値
    /// "C:\\Users\\Conaca\\AppData\\Roaming\\com.conaca.workfile-backupper\\config.json"
    fn get_path(app_handle: &AppHandle) -> PathBuf {
        app_handle
            .path()
            .app_config_dir()   // OS標準の設定フォルダを取得
            .expect("設定ファイルの保存先の取得に失敗")
            .join("config.json")
    }


    /// 設定をファイルに保存する
    pub fn save(&self, app_handle: &AppHandle) -> Result<(), String> {

        let path = Self::get_path(app_handle);

        // 親ディレクトリが無ければ作成
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        // JSON化して書込み (serde_json クレートを使用)
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())?;

        Ok(())
    }


    /// ファイルから設定を読み込む (読み込めなければErr)
    pub fn load(app_handle: &AppHandle) -> Result<Self, std::io::Error> {

        let path = Self::get_path(app_handle);
        
        // ファイルを読込、JSON→Configへ変換
        fs::read_to_string(path)
            .and_then(|content| serde_json::from_str(&content)
            .map_err(|e| e.into()))
    }


    /// 設定値が有効かをチェック
    /// 
    /// フォルダ監視の開始前に実行する必要がある
    /// パスフィールドを正規化し上書きするため、mut が必要
    pub fn validate(&mut self) -> Result<(), ConfigError> {
        use ConfigError::*;

        // 監視するフォルダ
        // canonicalize: 正規化 (絶対パス化 + 余計な/や.を削除)
        self.source_path = match self.source_path.canonicalize() {
            Ok(path) => {
                if path.is_dir() { path }
                else { return Err(InvalidSourcePath); }
            }
            Err(error) => {
                eprintln!("{error}");
                return Err(InvalidSourcePath);
            }
        };
        
        // バックアップ先フォルダ
        self.destination_path = match self.destination_path.canonicalize() {
            Ok(path) => {
                if path.is_dir() { path }
                else { return Err(InvalidDestinationPath); }
            }
            Err(error) => {
                eprintln!("{error}");
                return Err(InvalidDestinationPath);
            }
        };

        // バックアップ元とバックアップ先が同じ
        if self.source_path == self.destination_path {
            return Err(PathConflict);
        }

        // バックアップするファイル
        if !self.all_files_enabled &&     // 「全てのファイル」がfalse
            self.extensions.len() < 1 {   // 拡張子がひとつも登録されていない
            return Err(NoExtension);
        }

        // 未保存の通知
        if self.is_notify_unsaved &&      // 通知が有効
            self.notify_interval < 1 {    // 1分以下
            return Err(InvalidNotifyInterval);
        }

        Ok(())
    }
}

//=============================================================================
// テスト
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        let config = Config {
            source_path: PathBuf::from(r"D:\一時作業ファイル"),
            destination_path: PathBuf::from(r"E:\old【一時作業】"),
            recursive: true,
            is_notify: true,
            is_notify_unsaved: false,
            notify_interval: 30,
            is_shown: true,
            auto_start: false,
            all_files_enabled: false,
            extensions: [
                "psd",
                "sai2",
                "txt",
                "tmp",  // ファイル消失テスト
                "PpP",  // 大文字小文字テスト
            ].iter().map(|str| str.to_string()).collect()
        };

        println!("{:#?}", config);
        // config.save(app_handle);

        // assert_eq!(
        //     vec!["safe, fast, productive."],    // 正しい検索結果
        //     search(query, contents)             // "duct" が含まれる行を検索
        // );
    }
}