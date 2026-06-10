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

        // ファイルかどうかチェック
        // すでにファイルが無い場合もfalse (.tmpがすぐ削除された場合など)
        // ※環境に依存するため、テストから除外
        if !path.is_file() {
            return false;
        }

        // テスト可能な部分
        self.is_target_logic(path)
    }


    /// パスがバックアップ対象かを判定 (テスト可能な部分)
    fn is_target_logic(&self, path: &Path) -> bool {

        // 「全てのファイル」の場合: ファイルチェック済みのためtrue
        if self.all_files_enabled {
            return true;
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

//=============================================================================
// テスト
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// 「全てのファイル」の場合のテスト
    #[test]
    fn test_all_files_enabled() {
        test_base(true);
    }

    /// [拡張子で指定」の場合のテスト
    #[test]
    fn test_extension_filtering() {
        test_base(false);
    }


    /// テストの共通部分
    fn test_base(all_files_enabled: bool) {

        //------------------------------
        // テストケースを生成
        //------------------------------

        // 「拡張子で指定」項目の値
        let extensions = [
            "aaa",
            "BBB",           // 大文字 → 小文字に統一
            "CcC",           // 大文字小文字混合 → 小文字に統一
            "hidden_file",   // 隠しファイルのテスト用
        ];

        // 全OS共通のテストケース
        //  タプルの内容
        //   0: 「すべてのファイル」の場合の期待値
        //   1: 「拡張子で指定」の場合の期待値 (上記extensionsを使用)
        //   2: メソッドに渡すパス
        let mut test_cases = vec![
            (true, true,  "/absolute/folder/file.aaa"), // 絶対パス
            (true, true,  "relative/folder/file.aaa"),  // 相対パス
            (true, false, "no_extension_file"),         // 拡張子無し
            (true, false, "unregistered_ext.zzz"),      // 登録外の拡張子
            (true, false, ".hidden_file"),              // 隠しファイル (ドットから始まる)

            (true, true,  "file.AAA"),     // path が小文字化されるかチェック
            (true, true,  "file.bbb"),     // extensions が小文字化されるかチェック
            (true, true,  "file.cCc"),     // 大文字小文字混合チェック
        ];

        // Windows環境でのみ追加するケース
        if cfg!(target_os = "windows") {
            test_cases.extend(vec![
                (true, true,  r"C:\absolute\folder\file.aaa"),  // 絶対パス
                (true, true,  r"rerative\folder\file.aaa"),     // 相対パス
                (true, true,  r"d:\data\file.BBB"),             // ドライブレター小文字
                (true, false, r"C:\folder\no_extension_file"),  // 拡張子なし + 絶対パス
                (true, true,  r"C:\Mixed/Slashes/file.aaa"),    // 区切り文字は混在可能
            ]);
        }

        // &str → String に変換
        let extensions: Vec<String>
            = extensions.iter().map(|s| String::from(*s)).collect();

        //------------------------------
        // テストを実行
        //------------------------------

        let checker = TargetChecker::new(all_files_enabled, &extensions);

        for (expected_all, expected_ext, path_str) in test_cases {

            // 期待値を選択 (全てのファイル or 拡張子で指定)
            let expected = if all_files_enabled {expected_all} else {expected_ext};

            // テスト実行
            let result = checker.is_target_logic(Path::new(path_str));
            assert_eq!(result, expected, "パス「{}」で失敗", path_str);
        }
    }
}
