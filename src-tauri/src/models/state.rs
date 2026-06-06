use std::sync::Mutex;
use crate::models::config::Config;
use crate::services::utilities::lock_mutex;


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
    pub fn write(&self, config: Config) {
        * lock_mutex(&self.config) = config;
    }
    
    /// Stateから値をクローンする
    pub fn load(&self) -> Config {
        lock_mutex(&self.config).clone()
    }
}
