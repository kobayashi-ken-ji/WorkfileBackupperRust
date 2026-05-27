
/// 設定保存用構造体
struct Config {
    source_path: String,        // バックアップ元フォルダ
    destination_path: String,   // バックアップ先フォルダ
    extensions: Vec<String>,    // バックアップするファイルの種類 (拡張子)
    is_shown: bool,             // アプリ起動時にウィンドウを表示する
    is_notify: bool,            // デスクトップ通知をする
}