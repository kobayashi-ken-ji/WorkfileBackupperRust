use std::fs;
use std::path::{PathBuf, Path};
use chrono::{DateTime, Local};
use crate::models::notify::BackupResult;


/// 指定ファイルをバックアップ
/// 
/// コピーしたファイルには、[YYYYMMDD_HHMMSS]形式のタイムスタンプが付与される
/// 「サブフォルダを含める」に対応 (フォルダが無ければ生成される)
/// フォルダを指定すると CopyFailed を返す
/// 
/// # 引数
/// * `source` - コピー元フォルダ
/// * `destination` - コピー先フォルダ
/// * `target` - コピーするファイル (コピー元フォルダ以下の階層にあること)
/// 
/// # 戻り値
/// バックアップ処理の結果値
/// 
pub fn back_up_file(source: &Path, destination: &Path, target: &Path) -> BackupResult {
    use BackupResult::*;

    //-----------------------------------------------------
    // タイムスタンプ文字列を生成
    //-----------------------------------------------------

    // メタデータの取得
    let metadata = match fs::metadata(target) {
        Ok(metadata) => metadata,
        Err(error) => {
            eprintln!("{error}");
            return MetadataFailed(target.to_path_buf());
        }
    };

    // 最終更新時を取得
    let system_time = match metadata.modified() {
        Ok(time) => time,
        Err(error) => {
            eprintln!("{error}");
            return ModifiedFailed(target.to_path_buf());
        }
    };

    // 更新時 → ローカルタイムゾーン → [YYYYMMDD_HHMMSS]形式 へ変換
    let local_time: DateTime<Local> = DateTime::from(system_time);
    let time_stamp = local_time.format("[%Y%m%d_%H%M%S]").to_string();

    //-----------------------------------------------------
    // 保存先フォルダを生成
    //-----------------------------------------------------

    let destination = create_destination_folder(source, destination, target);

    //-----------------------------------------------------
    // ファイル名を生成
    //-----------------------------------------------------
    /*
        ファイル名の構築は OsString を使用する

            ファイル名にドットが含まれる場合
                1.00 というファイル名に PatuBuf::set_extension() すると
                00 を上書きしてしまう

            Windowsの拡張パスの場合
                PathBuf::push() すると拡張子のドットをフォルダ階層にしてしまう
     */

    // ファイル名を取得
    let Some(file_stem) = target.file_stem() else {
        return InvalidFileName(target.to_path_buf());
    };

    // ファイル名 + [YYYYMMDD_HHMMSS]
    let mut new_file_name = file_stem.to_os_string();
    new_file_name.push(time_stamp);

    // 拡張子がある場合は付与
    if let Some(extention) = target.extension() {
        new_file_name.push(".");
        new_file_name.push(extention);
    }

    // パスを生成 (ディレクトリ/新ファイル名)
    let mut new_path = destination;
    new_path.push(new_file_name);

    //-----------------------------------------------------
    // バックアップ処理
    //-----------------------------------------------------

    // 同名のファイルが既に存在するか確認
    if new_path.exists() {
        return AlreadyExists(new_path);
    }

    // バックアップ実行 ※ファイル専用関数
    match fs::copy(target, &new_path) {
        Ok(_) => Copied(new_path),
        Err(error) => {
            eprintln!("{error}");
            return CopyFailed(new_path);
        }
    }
}


/// コピー先のフォルダパスを算出し、フォルダを生成
/// 
/// 「サブフォルダを含む」に対応するための機能
/// 問題発生時は destination をそのまま返す
/// 
/// # 引数
/// * `source` - コピー元フォルダ
/// * `destination` - コピー先フォルダ
/// * `target` - コピーするファイル (コピー元フォルダ以下の階層にあること)
/// 
/// # 例
/// コピー元フォルダ: /source/ 
/// コピー先フォルダ: /dest/ 
/// コピーするファイル: /source/folder/file.txt 
/// 戻り値: /dest/folder/  ※ 「folder」を検出し、コピー先に結合
/// 
fn create_destination_folder(source: &Path, destination: &Path, target: &Path)
    -> PathBuf {

    // この時点で正規化されている
    // println!("検出したファイル: {:?}", target);

    // 親ディレクトリを抽出 (ファイル名の除去)
    let Some(parent) = target.parent() else {
        eprintln!("親ディレクトリの取得に失敗: {:?}", target);
        return PathBuf::from(destination);
    };

    // 相対パスを抽出 (監視フォルダ部分の除去)
    let relative_path = match parent.strip_prefix(source) {
        Ok(p) => p,
        Err(error) => {
            eprintln!("相対パス化に失敗: {error}");
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

//=============================================================================
// テスト
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::discriminant;
    use crate::models::notify::{ToNotify};

    #[test]
    fn test_back_up_file() {

        //-------------------------------------------------
        // テスト環境を構築
        //-------------------------------------------------

        // OSの安全な場所に、テスト専用一時フォルダを作成
        let tmp_dir = tempfile::tempdir().unwrap();

        // コピー元/先フォルダを作成
        let source_path      = tmp_dir.path().join("src");
        let destination_path = tmp_dir.path().join("dest");
        fs::create_dir_all(&source_path).unwrap();
        fs::create_dir_all(&destination_path).unwrap();

        // テスト用サブフォルダを作成
        let folder1 = source_path.join("folder1");
        let folder2 = folder1.join("folder2");
        fs::create_dir_all(&folder2).unwrap();

        // ファイルを作成
        let files = [
            source_path.join("exist.txt"),
            source_path.join("no_ext"),
            source_path.join(".hidden_file"),
            source_path.join("double.dot.txt"),
            folder1.join("exist_1.txt"),
            folder2.join("exist_2.txt"),
        ];

        for file in files {
            // println!("{:?}", file);
            std::fs::File::create(&file).unwrap();
            assert!(file.is_file(), "テスト用ファイルの生成に失敗: {:?}", file);
        }

        //-------------------------------------------------
        // テストケースを作成
        //-------------------------------------------------

        let src = source_path.clone();

        // テストするファイル名、結果値 を同時に定義
        use BackupResult::*;
        let cases = [
            Copied(src.join("exist.txt")),         // 存在するファイル
            Copied(src.join("no_ext")),            // 拡張子なし
            Copied(src.join(".hidden_file")),      // 隠しファイル
            Copied(src.join("double.dot.txt")),    // 拡張子以外にもドットを含む
            AlreadyExists(src.join("exist.txt")),  // 2回目なので、既にバックアップ済み

            // 階層テスト
            Copied(folder1.join("exist_1.txt")),
            Copied(folder2.join("exist_2.txt")),

            // フォルダはコピーに失敗
            // ※発生しない想定 (事前にTargetCheckerで排除)
            CopyFailed(folder1.clone()),

            // 存在しないファイルは、メタデータ取得に失敗
            // ※発生しない想定 (事前にTargetCheckerで排除)
            MetadataFailed(src.join("not_exist")),
            MetadataFailed(folder1.join("not_exist")),
            MetadataFailed(folder2.join("not_exist")),
        ];

        //-------------------------------------------------
        // ケースごとのテスト
        //-------------------------------------------------

        for expected in cases {

            // テストするファイルパスを抽出
            let target_path = expected.get_path();

            // バックアップ実行
            let result = back_up_file(&source_path, &destination_path, &target_path);
            // println!("{:?}\n{:?}\n", expected, result);

            // バリアントが同じかチェック
            assert_eq!(
                discriminant(&result),
                discriminant(&expected),
                "バリアントが不一致:\n{:?}\n{:?}",
                result,
                expected
            );

            // ファイル名が一致 (タイムスタンプと拡張子を除く)
            let result_path = result.get_path();
            let result_stem = result_path.file_stem().unwrap().to_string_lossy().to_string();
            let target_stem = target_path.file_stem().unwrap().to_string_lossy().to_string();
            assert!(
                result_stem.starts_with(&target_stem),
                "ファイル名が不一致 (タイムスタンプと拡張子を除く):\n{}\n{}",
                result_stem, target_stem
            );
            // println!("{}\n{}\n", result_stem, target_stem);

            // ファイルが実在するかチェック
            if let Copied(path) = result {
                assert!(path.is_file(), "コピーしたファイルが存在しない: {:?}", path);
            }
        }
    }

    #[test]
    fn test_create_destination_folder() {

        // テスト条件 (引数の固定部分)
        let source_path      = "src";
        let destination_path = "dest";

        // テストケース
        //      タプルの内容は
        //      0: メソッドの path に渡されるファイルパス
        //      1: 期待される戻り値 (フォルダパス)
        let cases = [
            ("src/file.txt", "dest"),
            ("src/folder1/file.txt", "dest/folder1"),
            ("src/folder1/folder2/file.txt", "dest/folder1/folder2"),
        ];

        for (target, expected) in cases {

            //-------------------------------------------------
            // テスト環境を構築
            //-------------------------------------------------

            // OSの安全な場所に、テスト専用一時フォルダを作成
            // スコープを抜ける際に削除される
            let tmp_dir = tempfile::tempdir().unwrap();

            // コピー元/先フォルダを作成
            let source      = tmp_dir.path().join(source_path);
            let destination = tmp_dir.path().join(destination_path);
            fs::create_dir_all(&source).unwrap();
            fs::create_dir_all(&destination).unwrap();

            //-------------------------------------------------
            // テスト実行・検証
            //-------------------------------------------------

            let target = tmp_dir.path().join(target);
            let result = create_destination_folder(&source, &destination, &target);

            let expected = tmp_dir.path().join(expected);
            assert_eq!(result, expected, "コピー先のフォルダパス算出に失敗");
            // println!("{:?}\n{:?}\n", result, expected);
        }
    }
}