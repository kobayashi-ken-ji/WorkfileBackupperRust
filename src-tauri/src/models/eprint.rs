
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
    fn test() {
        let result: Result::<(), &str> = Err("エラー発生理由");
        result.eprint("エラータイトル");
    }
}