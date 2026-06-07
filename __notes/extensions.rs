//=============================================================================
// 拡張子のリスト (バックアップ対象の判別用)
//=============================================================================

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path};


// 拡張子のリスト (バックアップ対象の判別用)
pub struct Extensions {
    extensions: HashSet<OsString>,
}

impl Extensions {
    
    /// ファイルの拡張子が登録に含まれているかチェック (大文字小文字を区別しない)
    pub fn contains(&self, path: &Path) -> bool {
        match path.extension() {
            Some(extension) => self.extensions.contains(&extension.to_ascii_lowercase()),
            None => false
        }
    }
}


// Configの拡張子リストからインスタンスを生成
// ※ Vec::as_slice() してから渡す必要あり
impl From<&[String]> for Extensions {
    fn from(source: &[String]) -> Self {

        // 小文字化 + OsString化
        let extensions: HashSet<OsString> = source.iter()
            .map(|extension| extension.to_ascii_lowercase().into())
            .collect();
        // println!("{:#?}", extensions);
        Self {extensions}
    }
}
