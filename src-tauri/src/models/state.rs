use std::sync::mpsc::Sender;
use super::message::NotifyDTO;

/// TauriのStateに登録するための構造体 (送信機をラップ)
pub struct MessageSender {
    pub tx: Sender<NotifyDTO>
}