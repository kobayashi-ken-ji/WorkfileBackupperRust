use std::sync::mpsc::Sender;
use super::message::NotifyMessage;

/// TauriのStateに登録するための構造体 (送信機をラップ)
pub struct MessageSender {
    pub tx: Sender<NotifyMessage>
}