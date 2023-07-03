pub mod runtime;
pub mod misc;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let a = 123;
        let b = 12323;
        runtime::test_async(async move {
            println!("test {} {}", a, b);
        });
    }
}
