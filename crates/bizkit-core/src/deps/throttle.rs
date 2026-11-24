use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tokio::time::Instant;

use super::{Mail, MailError, Mailer};

/// Caps how fast another mailer sends: each send starts at least
/// `1 / rate` seconds after the one before, across every caller. It waits
/// instead of dropping mail.
#[derive(Debug)]
pub struct ThrottledMailer {
    inner: Arc<dyn Mailer>,
    gap: Duration,
    next: Mutex<Option<Instant>>,
}

impl ThrottledMailer {
    /// At most `rate_per_second` sends a second through `inner`. A rate of 0
    /// counts as 1.
    #[must_use]
    pub fn new(inner: Arc<dyn Mailer>, rate_per_second: u32) -> Self {
        let gap = Duration::from_secs(1) / rate_per_second.max(1);
        Self { inner, gap, next: Mutex::new(None) }
    }

    /// Books the next free start time. The lock is released before the
    /// wait, so a slow send never holds up the next one's booking.
    async fn slot(&self) -> Instant {
        let mut next = self.next.lock().await;
        let now = Instant::now();
        let start = next.map_or(now, |at| at.max(now));
        *next = Some(start + self.gap);
        start
    }
}

#[async_trait]
impl Mailer for ThrottledMailer {
    async fn send(&self, mail: &Mail) -> Result<Option<String>, MailError> {
        tokio::time::sleep_until(self.slot().await).await;
        self.inner.send(mail).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deps::LogMailer;

    fn mail() -> Mail {
        Mail {
            to: "ada@example.com".to_owned(),
            subject: "s".to_owned(),
            text: "t".to_owned(),
            html: None,
            unsubscribe: None,
            id: None,
        }
    }

    #[tokio::test(start_paused = true)]
    async fn spaces_sends_by_the_rate() {
        let mailer = Arc::new(ThrottledMailer::new(Arc::new(LogMailer::new("a@b.c")), 4));
        let start = Instant::now();
        let sends = (0..5).map(|_| {
            let mailer = Arc::clone(&mailer);
            tokio::spawn(async move {
                mailer.send(&mail()).await.unwrap();
                Instant::now()
            })
        });
        let mut done = Vec::new();
        for send in sends {
            done.push(send.await.unwrap() - start);
        }
        done.sort();
        assert_eq!(
            done,
            [0, 250, 500, 750, 1000].map(Duration::from_millis),
            "one send every 250 ms"
        );
    }
}
