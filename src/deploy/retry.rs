//! Retrying a request a host failed for a reason that may pass.

use std::thread;
use std::time::Duration;

use crate::error::DeployError;

/// How many times a request is tried, and how long the wait before the second
/// try is; each later wait doubles.
pub(super) struct Backoff {
    attempts: u32,
    first: Duration,
}

impl Backoff {
    pub(super) const DEFAULT: Self = Self {
        attempts: 4,
        first: Duration::from_millis(250),
    };

    /// `attempt` until it succeeds, fails for a reason no retry changes, or the
    /// tries run out, in which case the last error is the one reported.
    pub(super) fn run<T>(
        &self,
        mut attempt: impl FnMut() -> Result<T, DeployError>,
    ) -> Result<T, DeployError> {
        let mut wait = self.first;
        for tried in 1.. {
            match attempt() {
                Err(error) if error.transient() && tried < self.attempts => {
                    tracing::debug!(%error, tried, "retrying");
                    thread::sleep(wait);
                    wait *= 2;
                }
                done => return done,
            }
        }
        unreachable!("the loop returns on its last attempt")
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::time::Duration;

    use super::Backoff;
    use crate::error::DeployError;
    use crate::error::deploy::Method;

    const INSTANT: Backoff = Backoff {
        attempts: 3,
        first: Duration::ZERO,
    };

    fn status(code: u16) -> DeployError {
        DeployError::request(Method::Put, "/k", code, "")
    }

    #[test]
    fn a_transient_failure_is_tried_again_until_it_passes() {
        let calls = Cell::new(0);
        let result = INSTANT.run(|| {
            calls.set(calls.get() + 1);
            if calls.get() < 3 {
                Err(status(503))
            } else {
                Ok(())
            }
        });
        assert!(result.is_ok());
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn the_last_error_is_reported_when_the_tries_run_out() {
        let calls = Cell::new(0);
        let result: Result<(), _> = INSTANT.run(|| {
            calls.set(calls.get() + 1);
            Err(status(500))
        });
        assert!(result.is_err());
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn a_failure_no_retry_changes_is_not_retried() {
        let calls = Cell::new(0);
        let result: Result<(), _> = INSTANT.run(|| {
            calls.set(calls.get() + 1);
            Err(status(403))
        });
        assert!(result.is_err());
        assert_eq!(calls.get(), 1);
    }
}
