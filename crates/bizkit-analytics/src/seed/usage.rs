//! The demo workspace's `bizkit` project: where the dev stack reports its
//! own usage (`self_usage` in `dev/bizkit.yaml`), seeded with two weeks of
//! made-up users so its insights open with data.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::Context as _;
use bizkit_core::DemoSeed;
use bizkit_core::db::Db;
use bizkit_core::ids::ProjectId;
use bizkit_core::time::UnixMillis;
use serde_json::json;

use super::{DAYS, Rng, arrival, mark_events, site_row};
use crate::api::Day;
use crate::files::{ProjectDb, project_db_path};
use crate::insights::{InsightQuery, seed_dashboard};
use crate::report::Scope;
use crate::store::{self, EventRow};

/// The demo project's slug, which `dev/bizkit.yaml` names.
const PROJECT: &str = "bizkit";
const DASHBOARD: &str = "bizkit usage";
const PEOPLE: u64 = 40;

/// The site, the seeded usage and the dashboard. The fill runs while the
/// file holds no seeded people, so live usage from the running api (which
/// starts once the site exists) can't make it skip, and a failed run
/// after the fill doesn't fill twice.
pub(crate) async fn seed(db: &Db, seed: &DemoSeed) -> anyhow::Result<()> {
    let Some(&project_id) = seed.project_ids.get(PROJECT) else { return Ok(()) };
    let file = ProjectDb::open(&project_db_path(&seed.data_dir, project_id))
        .await
        .context("opening the bizkit usage file")?;
    let seeded = fill_once(db, &file, seed, project_id).await;
    file.close().await;
    seeded?;
    let scope = Scope { workspace_id: seed.workspace_id, project_id };
    let mut tx = db.write().await.context("starting the usage dashboard write")?;
    if seed_dashboard(&mut tx, scope, seed.now, DASHBOARD, &insights()?)
        .await
        .context("adding the usage dashboard")?
    {
        tx.commit().await.context("committing the usage dashboard")?;
    }
    Ok(())
}

async fn fill_once(
    db: &Db,
    file: &ProjectDb,
    seed: &DemoSeed,
    project_id: ProjectId,
) -> anyhow::Result<()> {
    let filled: bool = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM analytics_people WHERE user_id LIKE 'seed_user_%') AS "filled!: bool""#,
    )
    .fetch_one(file.read())
    .await
    .context("checking for seeded usage")?;
    if !filled {
        fill(file, seed, project_id).await?;
    }
    if store::find_site(db.read(), seed.workspace_id, project_id).await?.is_none() {
        let mut tx = db.write().await.context("starting the usage site write")?;
        store::insert_site(&mut *tx, &site_row(seed, project_id, &json!([]), "always"))
            .await
            .context("adding the usage site")?;
        tx.commit().await.context("committing the usage site")?;
    }
    if !filled {
        mark_events(db, file, seed, project_id).await?;
    }
    Ok(())
}

fn insights() -> anyhow::Result<Vec<(&'static str, InsightQuery, u8)>> {
    [
        (
            "API key holders",
            json!({"kind": "people", "filters": [{"key": "api_keys", "op": "gt", "value": 0}],
                   "breakdown": {"source": "trait", "key": "api_keys"}}),
            4,
        ),
        (
            "Share creating API keys",
            json!({"kind": "events", "event": "api_key_created", "measure": "share"}),
            8,
        ),
        ("Sign-ins", json!({"kind": "events", "event": "signed_in", "measure": "users"}), 12),
    ]
    .into_iter()
    .map(|(name, query, width)| {
        Ok((name, serde_json::from_value(query).context("a usage insight")?, width))
    })
    .collect()
}

/// One made-up user's state while the seed walks the days.
struct User {
    id: String,
    workspace: Option<String>,
    workspaces: i64,
    api_keys: i64,
}

async fn fill(file: &ProjectDb, seed: &DemoSeed, project_id: ProjectId) -> anyhow::Result<()> {
    let today = Day::of(seed.now);
    let mut rng = Rng(0x5eed_0b12);
    let mut tx = file.write().await.context("starting the usage fill")?;
    let mut users: Vec<(i64, User)> = (0..PEOPLE)
        .map(|n| {
            let signup_age =
                i64::try_from(rng.below(u64::try_from(DAYS).unwrap_or(14))).unwrap_or(0);
            let user = User {
                id: format!("seed_user_{n:02}"),
                workspace: None,
                workspaces: 0,
                api_keys: 0,
            };
            (signup_age, user)
        })
        .collect();
    let mut events = Vec::new();
    for age in (0..DAYS).rev() {
        let day = today.plus_days(-age);
        for (signup_age, user) in &mut users {
            let mut at = arrival(&mut rng, day, seed.now);
            let mut push = |r: &mut Rng, who: &User, name: &str| {
                events.push(event(seed, project_id, UnixMillis(at.0.min(seed.now.0)), who, name));
                at = at.plus(Duration::from_secs(30 + r.below(900)));
            };
            if *signup_age == age {
                push(&mut rng, user, "signed_up");
                user.workspace = Some(format!("seed_ws_{:02}", rng.below(30)));
                user.workspaces = 1;
                if rng.chance(75) {
                    push(&mut rng, user, "workspace_created");
                    push(&mut rng, user, "project_created");
                } else {
                    push(&mut rng, user, "member_joined");
                }
            } else if *signup_age > age && rng.chance(45) {
                push(&mut rng, user, "signed_in");
                let action = rng.pick(&[
                    (5, ""),
                    (3, "project_created"),
                    (2, "invitation_sent"),
                    (2, "api_key_created"),
                    (1, "api_key_revoked"),
                ]);
                match action {
                    "api_key_created" => user.api_keys += 1,
                    "api_key_revoked" if user.api_keys > 0 => user.api_keys -= 1,
                    "api_key_revoked" | "" => continue,
                    _ => {}
                }
                push(&mut rng, user, action);
            }
        }
    }
    events.sort_by_key(|event| event.ts);
    let mut people = BTreeMap::new();
    for event in &mut events {
        let user_id = event.user_id.clone().unwrap_or_default();
        let id = store::upsert_person(&mut *tx, seed.workspace_id, project_id, &user_id, event.ts)
            .await?;
        event.person = format!("p:{id}");
        store::insert_event(&mut *tx, event).await.context("adding a usage event")?;
        people.insert(user_id, id);
    }
    for (_, user) in &users {
        let Some(&id) = people.get(&user.id) else { continue };
        let traits = json!({ "workspaces": user.workspaces, "api_keys": user.api_keys });
        store::set_person_traits(&mut *tx, seed.workspace_id, id, &traits.to_string())
            .await
            .context("setting usage traits")?;
    }
    tx.commit().await.context("committing bizkit usage")
}

/// Sign-ins carry no workspace, as live ones don't.
fn event(
    seed: &DemoSeed,
    project_id: ProjectId,
    at: UnixMillis,
    user: &User,
    name: &str,
) -> EventRow {
    EventRow {
        workspace_id: seed.workspace_id,
        project_id,
        ts: at,
        day: Day::of(at).to_string(),
        name: name.to_owned(),
        path: None,
        host: None,
        referrer_host: None,
        utm_source: None,
        utm_medium: None,
        utm_campaign: None,
        country: None,
        device: None,
        browser: None,
        os: None,
        visitor: 0,
        person: String::new(),
        anon_id: None,
        user_id: Some(user.id.clone()),
        props: user
            .workspace
            .as_ref()
            .filter(|_| !name.starts_with("signed_"))
            .map(|ws| json!({ "workspace_id": ws }).to_string()),
        source: "server",
    }
}
