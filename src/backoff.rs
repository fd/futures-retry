use rand::{thread_rng, Rng};
use std::time::Duration;

pub fn instant() -> impl Backoff + Sized {
    Duration::from_secs(0)
}

pub fn constant(duration: Duration) -> impl Backoff + Sized {
    duration
}

pub trait Backoff {
    fn next_retry(&mut self) -> Duration;

    fn exponential(self) -> Exponential<Self>
    where
        Self: Sized,
    {
        Exponential {
            factor: 1,
            inner: self,
        }
    }

    fn max_backoff(self, max: Duration) -> Max<Self>
    where
        Self: Sized,
    {
        Max { max, inner: self }
    }

    fn min_backoff(self, min: Duration) -> Min<Self>
    where
        Self: Sized,
    {
        Min { min, inner: self }
    }

    fn jitter(self, scale: f64) -> Jitter<Self>
    where
        Self: Sized,
    {
        Jitter { scale, inner: self }
    }
}

impl Backoff for Duration {
    fn next_retry(&mut self) -> Duration {
        *self
    }
}

pub struct Exponential<S>
where
    S: Backoff,
{
    inner: S,
    factor: u32,
}

impl<S> Backoff for Exponential<S>
where
    S: Backoff,
{
    fn next_retry(&mut self) -> Duration {
        self.factor *= 2;
        self.inner.next_retry() * (self.factor as _)
    }
}

pub struct Max<S>
where
    S: Backoff,
{
    inner: S,
    max: Duration,
}

impl<S> Backoff for Max<S>
where
    S: Backoff,
{
    fn next_retry(&mut self) -> Duration {
        std::cmp::min(self.max, self.inner.next_retry())
    }
}

pub struct Min<S>
where
    S: Backoff,
{
    inner: S,
    min: Duration,
}

impl<S> Backoff for Min<S>
where
    S: Backoff,
{
    fn next_retry(&mut self) -> Duration {
        std::cmp::max(self.min, self.inner.next_retry())
    }
}

pub struct Jitter<S>
where
    S: Backoff,
{
    inner: S,
    scale: f64,
}

impl<S> Backoff for Jitter<S>
where
    S: Backoff,
{
    fn next_retry(&mut self) -> Duration {
        let next = self.inner.next_retry();
        let margin = Duration::from_secs_f64(next.as_secs_f64() * self.scale);
        thread_rng().gen_range(next - margin, next)
    }
}
