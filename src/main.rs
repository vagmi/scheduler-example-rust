use tokio::time::{Duration, Instant, Sleep};
use anyhow::{Result};
use tokio::sync::oneshot::{Receiver, Sender, channel};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use pin_project::pin_project;

pub struct Canceller {
    tx: Sender<()>
}

impl Canceller {
    pub fn new() -> (Self, Receiver<()>) {
        let (tx, rx) = channel::<()>();
        (Canceller{tx}, rx)
    }
    pub fn cancel(self) -> Result<()> {
        self.tx.send(()).map_err(|_| { anyhow::format_err!("task send failed") })
    }
}

#[pin_project]
pub struct Job<T: Future>  {
    #[pin]
    value: T,
    #[pin]
    timer_future: Sleep,
    #[pin]
    rx: Receiver<()>,

    at: Instant,
}

impl<T> Job<T>
where T: Future {
    pub fn new(at: Instant, value: T, cancel_chan: Receiver<()>) -> Self {
        let timer_future = tokio::time::sleep_until(at);
        Job{ value, at, timer_future, rx: cancel_chan}
    }
}

impl<T: Future> Future for Job<T> {
    type Output = Result<T::Output>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = self.project();
        match me.rx.poll(cx) {
            Poll::Ready(_) => {
                println!("cancelling...");
                Poll::Ready(Err(anyhow::format_err!("cancelled")))
            },
            Poll::Pending => match me.timer_future.poll(cx) {
                Poll::Ready(_) => match me.value.poll(cx) {
                    Poll::Ready(x) => Poll::Ready(Ok(x)),
                    Poll::Pending => Poll::Pending
                },
                Poll::Pending => Poll::Pending
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let should_cancel = match std::env::args().nth(1) {
        Some(x) => x == "--cancel",
        _ => false
    };

    println!("started ... will execute in 5 seconds");

    let (canceller, cancel_chan) = Canceller::new();

    let j = Job::new(Instant::now() + Duration::from_secs(5), async {
        println!("5 seconds later")
    }, cancel_chan);
    let jh = tokio::spawn(j);

    if should_cancel {
        println!("ok. cancelling 3 seconds");
        tokio::time::sleep(Duration::from_secs(3)).await;
        canceller.cancel().unwrap();
    }
    jh.await.unwrap().unwrap();
    Ok(())
}