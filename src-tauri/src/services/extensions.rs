//=============================================================================
// 拡張子のリスト (バックアップ対象の判別用)
//=============================================================================

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub struct Extensions {
    extensions: HashSet<OsString>,
}

impl Extensions {

    /// コンストラクタ
    pub fn new() -> Self {
        Self { extensions: HashSet::new() }
    }
    
    /// 拡張子を登録 (自動で小文字化)
    pub fn insert(&mut self, extension: &str) {
        let extension = extension.to_ascii_lowercase();
        self.extensions.insert(extension.into());
    }

    /// 全要素を削除
    pub fn clear(&mut self) {
        self.extensions.clear();
    }
    
    /// ファイルの拡張子が登録に含まれているかチェック (大文字小文字を区別しない)
    pub fn contains(&self, path: &Path) -> bool {
        match path.extension() {
            Some(extension) => self.extensions.contains(&extension.to_ascii_lowercase()),
            None => false
        }
    }
}
