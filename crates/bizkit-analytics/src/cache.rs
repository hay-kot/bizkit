//! Caches that keep reads off the ingest path: sites (with their goals) by
//! public key, and today's salt.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use anyhow::Context as _;
use bizkit_core::db::Db;
use bizkit_core::time::UnixMillis;
use rand::RngExt as _;

use crate::api::{Day, IdentityMode};
use crate::store::{self, GoalRow, SiteRow};

/// How long a site stays cached. Settings and goal changes also drop it.
pub(crate) const SITE_TTL: Duration = Duration::from_secs(30);
/// Past this many cached keys (most of them unknown keys from junk
/// traffic), the cache starts over.
const MAX_SITES: usize = 10_000;

/// A site as ingest needs it.
#[derive(Debug, Clone)]
pub(crate) struct CachedSite {
    pub(crate) row: SiteRow,
    pub(crate) hostnames: Vec<String>,
    pub(crate) identity_mode: IdentityMode,
    pub(crate) goals: Vec<GoalRow>,
}

type Entry = (UnixMillis, Option<Arc<CachedSite>>);

/// Sites by public key. Unknown keys are cached too, as `None`.
#[derive(Debug, Default)]
pub(crate) struct Sites {
    map: Mutex<HashMap<String, Entry>>,
}

impl Sites {
    /// The site with public key `key`, if there is one.
    pub(crate) async fn get(
        &self,
        db: &Db,
        key: &str,
        now: UnixMillis,
    ) -> anyhow::Result<Option<Arc<CachedSite>>> {
        let cached = self.lock().get(key).cloned();
        if let Some((loaded, site)) = cached
            && now.0.saturating_sub(loaded.0) < i64::try_from(SITE_TTL.as_millis()).unwrap_or(0)
        {
            return Ok(site);
        }
        let site = load(db, key).await?.map(Arc::new);
        {
            let mut map = self.lock();
            if map.len() >= MAX_SITES {
                map.clear();
            }
            map.insert(key.to_owned(), (now, site.clone()));
        }
        Ok(site)
    }

    /// Drops every cached site, after a settings or goal change.
    pub(crate) fn clear(&self) {
        self.lock().clear();
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Entry>> {
        self.map.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

async fn load(db: &Db, key: &str) -> anyhow::Result<Option<CachedSite>> {
    let Some(row) = store::find_site_by_key(db.read(), key).await.context("finding a site")? else {
        return Ok(None);
    };
    let goals = store::list_goals(db.read(), row.workspace_id, row.project_id)
        .await
        .context("loading a site's goals")?;
    let hostnames = serde_json::from_str(&row.hostnames)
        .with_context(|| format!("decoding the hostnames of project {}", row.project_id))?;
    let identity_mode = IdentityMode::from_stored(&row.identity_mode);
    Ok(Some(CachedSite { row, hostnames, identity_mode, goals }))
}

/// Today's salt, shared by every site.
#[derive(Debug, Default)]
pub(crate) struct Salts {
    today: Mutex<Option<(Day, [u8; 32])>>,
}

impl Salts {
    /// The salt for `day`, made (and older ones deleted) on the day's first call.
    pub(crate) async fn get(&self, db: &Db, day: Day) -> anyhow::Result<[u8; 32]> {
        if let Some((cached, salt)) = *self.today.lock().unwrap_or_else(PoisonError::into_inner)
            && cached == day
        {
            return Ok(salt);
        }
        let fresh: [u8; 32] = rand::rng().random();
        let mut tx = db.write().await?;
        let stored = store::day_salt(&mut tx, &day.to_string(), &fresh).await?;
        tx.commit().await?;
        let salt: [u8; 32] =
            stored.try_into().map_err(|_| anyhow::anyhow!("the salt for {day} isn't 32 bytes"))?;
        *self.today.lock().unwrap_or_else(PoisonError::into_inner) = Some((day, salt));
        Ok(salt)
    }
}
