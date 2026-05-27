// Prevents additional console window on Windows in release, DO NOT REMOVE!!
// Windows環境のリリースビルドの時はコンソールを表示しない (GUIアプリ属性にする)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    workfile_backupper_lib::run()
}
