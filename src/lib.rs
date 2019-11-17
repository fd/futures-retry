use futures_timer::Delay;
use pin_project::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

pub mod backoff;

use backoff::Backoff;

/// Retry a future until it succeeds.
pub fn retry<R, S>(task: R, scheduler: S) -> Retry<R>
where
    R: Retryable,
    R::Error: std::fmt::Debug,
    S: Backoff + 'static,
{
    Retry {
        retryable: task,
        scheduler: Box::new(scheduler),
        state: RetryState::Pending,
        trying_fut: None,
        waiting_fut: None,
    }
}

/// Retryable must be implemented for a task that can be retried any number of times.
///
/// All errors wil be reported with `report_error`. The default implementation will report
/// the error with `tracing::error!()`.
pub trait Retryable {
    type Item;
    type Error: std::fmt::Debug;
    type Future: Future<Output = Result<Self::Item, Self::Error>>;

    /// Setup a new attempt at completing the task.
    fn call(&self) -> Self::Future;

    /// Report the error of the last attempt to complete the task.
    fn report_error(&self, error: Self::Error, next_retry: Duration) {
        tracing::error!(
            "error after retry: {:?} (will retry in {:?})",
            error,
            next_retry
        );
    }
}

/// Retry is return by `retry`
#[pin_project]
pub struct Retry<R>
where
    R: Retryable,
    R::Error: std::fmt::Debug,
{
    retryable: R,
    scheduler: Box<dyn Backoff>,
    state: RetryState,

    #[pin]
    trying_fut: Option<R::Future>,

    #[pin]
    waiting_fut: Option<Delay>,
}

enum RetryState {
    Pending,
    Trying,
    Waiting,
}

impl<R> Future for Retry<R>
where
    R: Retryable,
    R::Error: std::fmt::Debug,
{
    type Output = R::Item;

    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();
        loop {
            *this.state = match this.state {
                RetryState::Pending => {
                    this.waiting_fut.set(None);
                    this.trying_fut.set(Some(this.retryable.call()));
                    RetryState::Trying
                }
                RetryState::Trying => {
                    match this.trying_fut.as_mut().as_pin_mut().unwrap().poll(ctx) {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(Ok(result)) => return Poll::Ready(result),
                        Poll::Ready(Err(err)) => {
                            let retry_after = this.scheduler.next_retry();

                            // log error
                            this.retryable.report_error(err, retry_after);

                            this.trying_fut.set(None);
                            this.waiting_fut.set(Some(Delay::new(retry_after)));
                            RetryState::Waiting
                        }
                    }
                }
                RetryState::Waiting => {
                    match this.waiting_fut.as_mut().as_pin_mut().unwrap().poll(ctx) {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(_) => RetryState::Pending,
                    }
                }
            };
        }
    }
}

impl<F, Fut, I, E> Retryable for F
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<I, E>>,
    E: std::fmt::Debug,
{
    type Item = I;
    type Error = E;
    type Future = Fut;

    fn call(&self) -> Self::Future {
        self()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
