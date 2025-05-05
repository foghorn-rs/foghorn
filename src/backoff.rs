use std::time::Duration;

pub async fn retry_fib<T>(mut f: impl AsyncFnMut() -> Option<T>) -> T {
    let (mut a, mut b) = (0, 100);

    loop {
        if let Some(t) = f().await {
            return t;
        }

        tokio::time::sleep(Duration::from_millis(b)).await;

        (a, b) = (b, 1_000_000.min(a + b));
    }
}
