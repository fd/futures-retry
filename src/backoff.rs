use rand::{thread_rng, Rng};
use std::time::{Duration, Instant};

/// Make a zero delay backoff
pub fn instant() -> impl Backoff + Sized {
    Duration::from_secs(0)
}

/// Make a constant duration backoff
pub fn constant(duration: Duration) -> impl Backoff + Sized {
    duration
}

pub trait Backoff: Send {
    /// Get the duration to wait for before attempting again
    fn next_retry(&mut self) -> Option<Duration>;

    /// Grow the backoff duration exponentially
    fn exponential(self) -> Exponential<Self>
    where
        Self: Sized,
    {
        Exponential {
            factor: 1,
            inner: self,
        }
    }

    /// Set the maximum backoff duration
    fn max_backoff(self, max: Duration) -> Max<Self>
    where
        Self: Sized,
    {
        Max { max, inner: self }
    }

    /// Set the minimum backoff duration
    fn min_backoff(self, min: Duration) -> Min<Self>
    where
        Self: Sized,
    {
        Min { min, inner: self }
    }

    /// Randomize the backoff duration.
    ///
    /// The returned duration will never be larger than the base duration and will
    /// never be smaller than `base * (1.0 - scale)`.
    fn jitter(self, scale: f64) -> Jitter<Self>
    where
        Self: Sized,
    {
        assert!(scale > 0.0, "scale must be larger than zero");
        assert!(scale <= 1.0, "scale must be smaller or equal to one");
        Jitter { scale, inner: self }
    }

    fn num_attempts(self, num: u32) -> MaxAttempts<Self>
    where
        Self: Sized,
    {
        assert!(num > 0, "num must be larger than zero");
        let num_attempts_left = num - 1;
        MaxAttempts {
            num_attempts_left,
            inner: self,
        }
    }

    fn deadline(self, deadline: Instant) -> Deadline<Self>
    where
        Self: Sized,
    {
        Deadline {
            deadline,
            inner: self,
        }
    }
}

impl Backoff for Duration {
    fn next_retry(&mut self) -> Option<Duration> {
        Some(*self)
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
    fn next_retry(&mut self) -> Option<Duration> {
        let dur = self.inner.next_retry().map(|dur| dur * (self.factor as _));
        self.factor *= 2;
        dur
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
    fn next_retry(&mut self) -> Option<Duration> {
        self.inner
            .next_retry()
            .map(|dur| std::cmp::min(self.max, dur))
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
    fn next_retry(&mut self) -> Option<Duration> {
        self.inner
            .next_retry()
            .map(|dur| std::cmp::max(self.min, dur))
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
    fn next_retry(&mut self) -> Option<Duration> {
        self.inner.next_retry().map(|dur| {
            let margin = Duration::from_secs_f64(dur.as_secs_f64() * self.scale);
            thread_rng().gen_range(dur - margin, dur)
        })
    }
}

pub struct MaxAttempts<S>
where
    S: Backoff,
{
    inner: S,
    num_attempts_left: u32,
}

impl<S> Backoff for MaxAttempts<S>
where
    S: Backoff,
{
    fn next_retry(&mut self) -> Option<Duration> {
        if self.num_attempts_left > 0 {
            self.num_attempts_left -= 1;
            self.inner.next_retry()
        } else {
            None
        }
    }
}

pub struct Deadline<S>
where
    S: Backoff,
{
    inner: S,
    deadline: Instant,
}

impl<S> Backoff for Deadline<S>
where
    S: Backoff,
{
    fn next_retry(&mut self) -> Option<Duration> {
        if self.deadline < Instant::now() {
            None
        } else {
            self.inner.next_retry()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instant() {
        let mut bo = instant();
        assert_eq!(bo.next_retry(), Some(Duration::from_secs(0)));
        assert_eq!(bo.next_retry(), Some(Duration::from_secs(0)));
    }

    #[test]
    fn test_constant() {
        let mut bo = constant(Duration::from_secs(5));
        assert_eq!(bo.next_retry(), Some(Duration::from_secs(5)));
        assert_eq!(bo.next_retry(), Some(Duration::from_secs(5)));
    }

    #[test]
    fn test_min_backoff() {
        let mut bo = constant(Duration::from_secs(5)).min_backoff(Duration::from_secs(10));
        assert_eq!(bo.next_retry(), Some(Duration::from_secs(10)));
        assert_eq!(bo.next_retry(), Some(Duration::from_secs(10)));

        let mut bo = constant(Duration::from_secs(5)).min_backoff(Duration::from_secs(3));
        assert_eq!(bo.next_retry(), Some(Duration::from_secs(5)));
        assert_eq!(bo.next_retry(), Some(Duration::from_secs(5)));
    }

    #[test]
    fn test_max_backoff() {
        let mut bo = constant(Duration::from_secs(5)).max_backoff(Duration::from_secs(10));
        assert_eq!(bo.next_retry(), Some(Duration::from_secs(5)));
        assert_eq!(bo.next_retry(), Some(Duration::from_secs(5)));

        let mut bo = constant(Duration::from_secs(5)).max_backoff(Duration::from_secs(3));
        assert_eq!(bo.next_retry(), Some(Duration::from_secs(3)));
        assert_eq!(bo.next_retry(), Some(Duration::from_secs(3)));
    }

    #[test]
    fn test_exponential() {
        let mut bo = constant(Duration::from_secs(1)).exponential();
        assert_eq!(bo.next_retry(), Some(Duration::from_secs(1)));
        assert_eq!(bo.next_retry(), Some(Duration::from_secs(2)));
        assert_eq!(bo.next_retry(), Some(Duration::from_secs(4)));
        assert_eq!(bo.next_retry(), Some(Duration::from_secs(8)));
    }

    #[test]
    fn test_jitter() {
        let mut bo = constant(Duration::from_secs(1)).jitter(0.1);
        let range = Duration::from_millis(900)..=Duration::from_secs(1);
        for _i in 0..100_000 {
            let dur = bo.next_retry().unwrap();
            assert!(range.contains(&dur));
        }
    }

    #[test]
    fn test_num_attempts() {
        let mut bo = constant(Duration::from_secs(1)).num_attempts(3);
        assert_eq!(bo.next_retry(), Some(Duration::from_secs(1)));
        assert_eq!(bo.next_retry(), Some(Duration::from_secs(1)));
        assert_eq!(bo.next_retry(), None);
        assert_eq!(bo.next_retry(), None);
    }

    #[test]
    fn deadline() {
        let mut bo =
            constant(Duration::from_secs(1)).deadline(Instant::now() + Duration::from_millis(20));
        assert_eq!(bo.next_retry(), Some(Duration::from_secs(1)));
        assert_eq!(bo.next_retry(), Some(Duration::from_secs(1)));
        std::thread::sleep(Duration::from_millis(21));
        assert_eq!(bo.next_retry(), None);
        assert_eq!(bo.next_retry(), None);
    }
}
