use std::sync::{Mutex, MutexGuard};


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
