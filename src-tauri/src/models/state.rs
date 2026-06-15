use std::sync::Mutex;
use crate::models::config::Config;
use crate::utilities::SafeMutex;


/// TauriのStateに設定値を登録するための構造体
pub struct ConfigState {
    config: Mutex<Config>,
}

impl ConfigState {

    /// Tauriのmanageで使用するコンストラクタ
    pub fn new() -> Self {
        Self { config: Mutex::new(Config::default()) }
    }

    /// 値を消費してStateへ上書きする
    pub fn set(&self, config: Config) {
        * self.config.safe_lock() = config;
    }
    
    /// Stateから値をクローンする
    pub fn get(&self) -> Config {
        self.config.safe_lock().clone()
    }
}
