use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::Manager;
use ts_rs::TS;
use crate::models::notify::ConfigError;

/// 設定ファイルの名前
/// OSごとに適したディレクトリへ保存される
const CONFIG_FILE_NAME: &str = "config.json";


/*
    TSへ送信するための設定
    #[derive(serde::Serialize, serde::Deserialize, TS)]     シリアライズ化(JSON化)
    #[serde(...)]  シリアライズ時、JSに合わせてキャメルケース化
    #[ts(...)]     型定義ファイルを出力する設定

    > cargo test --lib を行うと型定義ファイルが生成される
*/

/// アプリ全体のユーザー設定
/// 
/// * デフォルト値で生成する機能
/// * OSに合わせた場所へ、JSONで読み書きする機能
/// * 設定値のバリデーションチェック機能
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types.ts")]
pub struct Config {

    // バックアップ設定
    pub source_path: PathBuf,       // バックアップ元フォルダ (監視フォルダ)
    pub destination_path: PathBuf,  // バックアップ先フォルダ
    pub recursive: bool,            // サブフォルダを含める
    pub all_files_enabled: bool,    // 全てのファイルをバックアップする
    pub extensions: Vec<String>,    // バックアップするファイルを指定 (拡張子)

    // デスクトップ通知の設定
    pub is_notify: bool,            // バックアップ情報を通知する
    pub is_notify_unsaved: bool,    // ファイル未保存時間を通知する
    pub notify_interval: u32,       // 未保存時間の通知間隔 (分)

    // アプリ起動時の設定
    pub is_shown: bool,             // アプリ起動時にウィンドウを表示する
    pub auto_start: bool,           // アプリ起動時に自動的に開始する
}


impl Default for Config {

    /// デフォルト値で新規作成
    fn default() -> Self {
        if cfg!(any(debug_assertions, test)) {

            // デバッグとテストの時のみ
            Self {
                source_path: PathBuf::from(r"D:\一時作業ファイル"),
                destination_path: PathBuf::from(r"E:\old【一時作業】"),
                recursive: true,
                all_files_enabled: true,
                extensions: vec![
                    String::from("txt"),
                    String::from("psd"),
                    String::from("sai2"),
                ],
                is_notify: true,
                is_notify_unsaved: false,
                notify_interval: 30,
                is_shown: true,
                auto_start: false,
            }
        } else {

            // リリース時
            Self {
                source_path: PathBuf::from(""),
                destination_path: PathBuf::from(""),
                recursive: true,
                all_files_enabled: true,
                extensions: Vec::new(),
                is_notify: true,
                is_notify_unsaved: false,
                notify_interval: 30,
                is_shown: true,
                auto_start: false,
            }
        }
    }
}


impl Config {

    /// 設定ファイルの保存先パスを取得
    /// 
    /// OSに適切なディレクトリが選択される。
    /// Windows環境では
    /// "C:\\Users\\XXXXX\\AppData\\Roaming\\com.conaca.workfile-backupper\\config.json"
    fn get_path(app_handle: &AppHandle) -> PathBuf {
        app_handle
            .path()
            .app_config_dir()   // OS標準の設定フォルダを取得
            .expect("設定ファイルの保存先の取得に失敗")
            .join(CONFIG_FILE_NAME)
    }


    /// 設定をJSONファイルで保存する
    pub fn save(&self, app_handle: &AppHandle) -> Result<(), String> {
        let path = Self::get_path(app_handle);
        self.save_core(&path)
    }

    /// saveからAppHandle処理を排除した部分
    fn save_core(&self, path: &PathBuf) -> Result<(), String> {

        // 親ディレクトリが無ければ作成
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        // JSON化して書込み (serde_json クレートを使用)
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())?;

        Ok(())
    }


    /// 設定をJSONファイルから読み込む
    pub fn load(app_handle: &AppHandle) -> Result<Self, std::io::Error> {
        let path = Self::get_path(app_handle);
        Self::load_core(&path)
    }

    /// loadからAppHandle処理を排除した部分
    fn load_core(path: &PathBuf) -> Result<Self, std::io::Error> {
        
        // ファイルを読込、JSON→Configへ変換
        fs::read_to_string(path)
            .and_then(|content| serde_json::from_str(&content)
            .map_err(|e| e.into()))
    }


    /// 設定値が有効かをチェック
    /// 
    /// フィールドの PathBuf の正規化も行う。 
    /// フォルダ監視の開始前に実行する必要がある。 
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
    use std::env::temp_dir;

    #[test]
    fn test_config() {

        //-------------------------------------------------
        // テスト環境を構築
        //-------------------------------------------------

        // OSの安全な場所に、テスト専用一時フォルダを作成
        let tmp_dir = tempfile::tempdir().expect("一時フォルダの作成に失敗");

        // コピー元/先フォルダを作成
        let source_path      = tmp_dir.path().join("src");
        let destination_path = tmp_dir.path().join("dest");
        fs::create_dir_all(&source_path).unwrap();
        fs::create_dir_all(&destination_path).unwrap();

        // ファイルの保存先パスを生成
        let path = tmp_dir.path().join(CONFIG_FILE_NAME);

        //-------------------------------------------------
        // 保存/読込テスト 
        //------------------------------------------------- 

        let config = Config::default();
        
        // 設定保存テスト
        let result = config.save_core(&path);
        assert!(result.is_ok(), "設定保存テスト: {:?}", result.err());

        // 設定読込テスト
        let result = Config::load_core(&path);
        assert!(result.is_ok(), "設定読込テスト: {:?}", result.err());

        // 元の値と等しいかをテスト
        let loaded = result.unwrap();
        assert_eq!(&loaded, &config, "設定保存/読込の内容整合テスト");
        // println!("設定の内容整合テスト\n{:?}\n{:?}\n", loaded, config);

        //------------------------------------------------- 
        // バリデーションテスト
        //------------------------------------------------- 

        use ConfigError::*;

        // エラーの出ない値を生成
        let mut default_config = Config::default();
        default_config.source_path = source_path;
        default_config.destination_path = destination_path;
        default_config.extensions = vec!["txt".into()];
        
        // バックアップ「元」フォルダが存在しない場合のテスト
        let mut config = default_config.clone();
        config.source_path = "".into();
        assert!(
            matches!(config.validate(), Err(InvalidSourcePath)),
            "バックアップ元が存在しない場合のテスト"
        );

        // バックアップ「先」フォルダが存在しない場合のテスト
        let mut config = default_config.clone();
        config.destination_path = "".into();
        assert!(
            matches!(config.validate(), Err(InvalidDestinationPath)),
            "バックアップ先が存在しない場合のテスト"
        );

        // バックアップ元/先が同じ場合のテスト
        let mut config = default_config.clone();
        config.destination_path = config.source_path.clone();
        assert!(
            matches!(config.validate(), Err(PathConflict)),
            "バックアップ元/先が同じ場合のテスト"
        );

        // 拡張子が未登録の場合のテスト
        let mut config = default_config.clone();
        config.all_files_enabled = false;
        config.extensions = Vec::new();
        assert!(
            matches!(config.validate(), Err(NoExtension)),
            "拡張子が未登録の場合のテスト"
        );

        // 未保存の通知が1分未満の場合のテスト
        let mut config = default_config.clone();
        config.is_notify_unsaved = true;
        config.notify_interval = 0;
        assert!(
            matches!(config.validate(), Err(InvalidNotifyInterval)),
            "未保存の通知が1分未満の場合のテスト"
        );
    }
}