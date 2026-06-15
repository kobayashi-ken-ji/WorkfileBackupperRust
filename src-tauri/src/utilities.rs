//! 汎用的な処理を定義

use std::sync::{Arc, Mutex, MutexGuard};

//=============================================================================
// ミューテックス処理
//=============================================================================

/// Mutex へポイズンエラー処理機能を実装する
pub trait SafeMutex<T> {
    fn safe_lock(&self) -> MutexGuard<'_, T>;
}

impl<T> SafeMutex<T> for Mutex<T> {

    /// Mutex::lock() とポイズンエラー処理を行う
    /// ※呼出し元では素早くガードを解放すること
    fn safe_lock(&self) -> MutexGuard<'_, T> {
        const MUTEX_POISON_ERR: &str
            = "ミューテックスのポイズンエラーが発生。強制取得して続行します。";

        match self.lock() {
            Ok(guard) => guard,
            Err(poison_err) => {
                eprintln!("{}", MUTEX_POISON_ERR);
                poison_err.into_inner()
            }
        }
    }
}

//=============================================================================
// エラー処理
//=============================================================================

/// Result型のエラー出力を行う
pub trait ResutlErrPrint {
    fn eprint(self, message: &str);
}

// 既存のResult型にメソッドを実装
impl<T, E: std::fmt::Display> ResutlErrPrint for Result<T, E> {

    /// 値がErrの場合、エラー標準出力に出力する
    /// (expect のパニックを起こさない版)
    fn eprint(self, message: &str) {
        if let Err(error) = self {
            eprintln!("[ERROR] {}: {}", message, error);
        }
    }
}

//=============================================================================
// モック用の共通処理
//=============================================================================

// ※ 現在不使用

/// モックの呼び出し履歴を記録する、汎用スパイ
/// 
/// # 例
/// ```
/// # use workfile_backupper_lib::utilities::CallLog;
/// 
/// // クローンすることで、別の場所から履歴を確認できる
/// let log: CallLog<i32> = CallLog::new();
/// let log_clone = log.clone();
/// 
/// // 記録
/// log.push(5);
/// 
/// // クローン側から履歴を確認
/// assert_eq!(log_clone.last(), 5);
/// ```
#[derive(Clone, Default)]
pub struct CallLog<T: Clone> {
    calls: Arc<Mutex<Vec<T>>>,
}

impl<T: Clone> CallLog<T> {
    pub fn new() -> Self {
        Self { calls: Arc::new(Mutex::new(Vec::new())) }
    }

    /// 呼び出し履歴を追加する
    pub fn push(&self, item:T) {
        self.calls.safe_lock().push(item);
    }

    /// 記録された件数を取得する
    pub fn len(&self) -> usize {
        self.calls.safe_lock().len()
    }

    /// 記録された最後の要素を取得する
    /// ※要素が存在しない場合はパニックが発生する
    pub fn last(&self) -> T {
        let calls = self.calls.safe_lock();
        let last = calls.last().expect("要素数が0のため、取得に失敗");
        last.clone()
    }
}

//=============================================================================
// テスト
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eprint() {
        let result: Result::<(), &str> = Err("エラー発生理由がここに表示されます");
        result.eprint("エラー表示テスト");
    }
}