//! Graceful shutdown signal handling.
//!
//! [`SigDown`] listens for OS shutdown signals (SIGTERM/SIGINT on Unix,
//! Ctrl+C on Windows) and triggers a [`CancellationToken`] that can be
//! distributed to multiple subsystems for coordinated graceful shutdown.

#[cfg(unix)]
use tokio::signal::unix::SignalKind;
#[cfg(unix)]
use tokio::signal::unix::signal;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

/// Handles graceful shutdown on SIGTERM / SIGINT / Ctrl+C.
#[allow(missing_debug_implementations)]
pub struct SigDown {
    task_tracker: TaskTracker,
    cancellation_token: CancellationToken,
}

impl SigDown {
    /// Creates a new signal handler and spawns the background listener.
    ///
    /// # Errors
    ///
    /// Returns an [`std::io::Error`] if signal registration fails.
    #[allow(clippy::unnecessary_wraps)]
    pub fn try_new() -> Result<Self, std::io::Error> {
        let inner = CancellationToken::new();
        let outer = inner.clone();
        let task_tracker = TaskTracker::new();

        #[cfg(unix)]
        {
            let mut sigterm = signal(SignalKind::terminate())?;
            let mut sigint = signal(SignalKind::interrupt())?;
            task_tracker.spawn(async move {
                tokio::select! {
                    _ = sigterm.recv() => {
                        inner.cancel();
                    },
                    _ = sigint.recv() => {
                        inner.cancel();
                    }
                }
            });
        }

        #[cfg(windows)]
        {
            task_tracker.spawn(async move {
                let _ = tokio::signal::ctrl_c().await;
                inner.cancel();
            });
        }

        task_tracker.close();
        Ok(Self {
            task_tracker,
            cancellation_token: outer,
        })
    }

    /// Returns a clone of the cancellation token for distributing to subsystems.
    #[must_use]
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation_token.clone()
    }

    /// Waits for a shutdown signal and ensures the handler task completes.
    #[allow(dead_code)]
    pub async fn recv(&self) {
        self.cancellation_token.cancelled().await;
        self.task_tracker.wait().await;
    }
}
