use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path};

/// 「バックアップ対象かどうか」の判定機
pub struct TargetChecker {
    all_files_enabled: bool,        // 「全てのファイル」チェックボックスの値
    extensions: HashSet<OsString>,  // 「拡張子で指定」のリスト
}

impl TargetChecker {

    /// Configの値を受け取り、判定用に加工
    pub fn new(all_files_enabled: bool, extensions: &[String]) -> Self {

        // 小文字化 + OsString化
        let extensions: HashSet<OsString> = extensions.iter()
            .map(|extension| extension.to_ascii_lowercase().into())
            .collect();

        Self { all_files_enabled, extensions }
    }


    /// パスがバックアップ対象かを判定
    pub fn is_target(&self, path: &Path) -> bool {

        // 「全てのファイル」の場合: ファイルのみtrue
        if self.all_files_enabled {
            return path.is_file();
        }

        // 「拡張子で指定」の場合: リストに拡張子が含まれていればtrue  
        match path.extension() {
            None => false,
            Some(extension) => {

                // 小文字に統一して比較
                let lowercase = extension.to_ascii_lowercase();
                self.extensions.contains(&lowercase)
            }
        }
    }
}