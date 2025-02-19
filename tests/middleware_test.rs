mod middleware;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dummy_test() {
        // 이 파일이 존재하는 이유는 middleware 모듈을 등록하기 위함입니다.
        assert!(true);
    }
}