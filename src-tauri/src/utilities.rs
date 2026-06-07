//! 汎用的な処理を定義

use std::sync::{Mutex, MutexGuard};

//=============================================================================
// ミューテックス処理
//=============================================================================

/// ミューテックスのロックを解除し、ポイズンエラー対策を行う
/// ※呼出し元では素早くガードを解放すること
pub fn lock_mutex<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {

    const MUTEX_POISON_ERR: &str
        = "ミューテックスのポイズンエラーが発生。強制取得して続行します。";

    match mutex.lock() {
        Ok(guard) => guard,
        Err(poison_err) => {
            eprintln!("{}", MUTEX_POISON_ERR);
            poison_err.into_inner()
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