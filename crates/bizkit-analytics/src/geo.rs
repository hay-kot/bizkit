//! Countries from an optional IP-to-country database file, in the format
//! of `MaxMind` and `DB-IP`'s free country files. No file, or one that
//! doesn't load, means no country: analytics never depends on it.

use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;

use maxminddb::{Reader, geoip2};
use tokio::sync::OnceCell;

/// The country lookup. The file loads on the first lookup, off the async
/// runtime, so startup does no I/O.
#[derive(Debug, Default)]
pub struct Geo {
    path: Option<PathBuf>,
    reader: OnceCell<Option<Arc<Reader<Vec<u8>>>>>,
}

impl Geo {
    /// A lookup over the file at `path`, if any.
    #[must_use]
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path, reader: OnceCell::new() }
    }

    /// The ISO 3166-1 alpha-2 code of the country `ip` is in, when known.
    pub async fn country(&self, ip: IpAddr) -> Option<String> {
        let reader = self.reader.get_or_init(|| load(self.path.clone())).await.as_ref()?;
        let found = reader.lookup(ip).ok()?;
        let record = found.decode::<geoip2::Country<'_>>().ok()??;
        record.country.iso_code.map(str::to_owned)
    }
}

async fn load(path: Option<PathBuf>) -> Option<Arc<Reader<Vec<u8>>>> {
    let path = path?;
    let shown = path.display().to_string();
    let opened = tokio::task::spawn_blocking(move || Reader::open_readfile(path)).await;
    match opened {
        Ok(Ok(reader)) => {
            tracing::info!(path = %shown, "loaded the GeoIP database");
            Some(Arc::new(reader))
        }
        Ok(Err(err)) => {
            tracing::warn!(path = %shown, error = %err, "can't read the GeoIP database; countries stay unknown");
            None
        }
        Err(err) => {
            tracing::warn!(error = %err, "loading the GeoIP database failed");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn no_file_means_no_country() {
        let ip: IpAddr = "8.8.8.8".parse().unwrap();
        assert_eq!(Geo::new(None).country(ip).await, None);
        let missing = Geo::new(Some(PathBuf::from("/nonexistent/geo.mmdb")));
        assert_eq!(missing.country(ip).await, None);
        assert_eq!(missing.country(ip).await, None, "the failed load is kept");
    }
}
