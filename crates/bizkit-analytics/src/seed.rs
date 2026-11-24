//! Sample traffic for the dev stack's demo workspace: 14 days of visits on
//! both demo projects, goals, a signup funnel and identified people who come
//! back, so every report has something to show.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::Context as _;
use bizkit_core::DemoSeed;
use bizkit_core::db::Db;
use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::time::UnixMillis;
use serde_json::{Value, json};
use sqlx::SqliteConnection;

use crate::api::{Day, FunnelId, GoalId, PersonId};
use crate::files::{ProjectDb, project_db_path};
use crate::funnels::{self, FunnelRow};
use crate::hooks::GOAL;
use crate::store::{self, EventRow, GoalRow, SiteRow};

const DAYS: i64 = 14;
const SIGNUP_GOAL: &str = "Signup";
const ONBOARDING_FUNNEL: &str = "Onboarding";
/// The demo site (`bizkit dev demo-site`) is served from localhost.
const DEMO_SITE_HOST: &str = "localhost";

/// A small deterministic generator, so every seeded stack looks alike.
struct Rng(u64);

impl Rng {
    const fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// True with `percent` in 100.
    const fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }

    /// Picks from `(weight, item)` pairs.
    fn pick<T: Copy + Default>(&mut self, items: &[(u64, T)]) -> T {
        let total: u64 = items.iter().map(|(weight, _)| weight).sum();
        let mut roll = self.below(total);
        items
            .iter()
            .find(|(weight, _)| {
                let hit = roll < *weight;
                roll = roll.saturating_sub(*weight);
                hit
            })
            .or_else(|| items.last())
            .map_or_else(T::default, |(_, item)| *item)
    }

    const fn visitor(&mut self) -> i64 {
        i64::from_ne_bytes(self.next().to_ne_bytes())
    }
}

/// Where a visit came from: referrer host and UTM source, medium, campaign.
type Source =
    (Option<&'static str>, Option<&'static str>, Option<&'static str>, Option<&'static str>);

const SOURCES: [(u64, Source); 7] = [
    (35, (None, None, None, None)),
    (25, (Some("google.com"), None, None, None)),
    (10, (Some("news.ycombinator.com"), None, None, None)),
    (8, (Some("t.co"), Some("twitter"), Some("social"), Some("launch"))),
    (7, (Some("github.com"), None, None, None)),
    (8, (None, Some("newsletter"), Some("email"), Some("november"))),
    (7, (Some("duckduckgo.com"), None, None, None)),
];

const COUNTRIES: [(u64, Option<&str>); 8] = [
    (30, Some("US")),
    (12, Some("DE")),
    (10, Some("GB")),
    (8, Some("FR")),
    (8, Some("CA")),
    (8, Some("IN")),
    (6, Some("NL")),
    (6, None),
];

/// Device, browser and OS, as the UA parser names them.
const AGENTS: [(u64, Agent); 8] = [
    (24, ("desktop", "Chrome", "macOS")),
    (18, ("desktop", "Chrome", "Windows")),
    (12, ("desktop", "Safari", "macOS")),
    (6, ("desktop", "Firefox", "Linux")),
    (6, ("desktop", "Edge", "Windows")),
    (18, ("mobile", "Safari", "iOS")),
    (12, ("mobile", "Chrome", "Android")),
    (4, ("tablet", "Safari", "iOS")),
];

const LANDINGS: [(u64, &str); 6] = [
    (55, "/"),
    (15, "/blog/launch"),
    (10, "/pricing"),
    (10, "/features"),
    (5, "/about"),
    (5, "/blog/sqlite-analytics"),
];

const DOCS_PAGES: [(u64, &str); 6] = [
    (30, "/docs"),
    (25, "/docs/getting-started"),
    (15, "/docs/snippet"),
    (12, "/docs/api"),
    (10, "/docs/self-hosting"),
    (8, "/docs/changelog"),
];

const NAMES: [(u64, (&str, &str)); 12] = [
    (1, ("Ada Lovelace", "ada")),
    (1, ("Grace Hopper", "grace")),
    (1, ("Alan Turing", "alan")),
    (1, ("Katherine Johnson", "katherine")),
    (1, ("Edsger Dijkstra", "edsger")),
    (1, ("Barbara Liskov", "barbara")),
    (1, ("Donald Knuth", "donald")),
    (1, ("Margaret Hamilton", "margaret")),
    (1, ("Linus Torvalds", "linus")),
    (1, ("Frances Allen", "frances")),
    (1, ("Ken Thompson", "ken")),
    (1, ("Radia Perlman", "radia")),
];

/// One visitor's events on one day, before they are stored.
struct Visit {
    events: Vec<EventRow>,
    signed_up: Option<(String, Value)>,
    agent: Agent,
    country: Option<&'static str>,
    age: i64,
}

/// Device, browser and OS.
type Agent = (&'static str, &'static str, &'static str);

/// Everything shared by a site's events.
struct Site {
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    host: &'static str,
}

impl Site {
    fn event(&self, at: UnixMillis, name: &str, path: &str) -> EventRow {
        EventRow {
            workspace_id: self.workspace_id,
            project_id: self.project_id,
            ts: at,
            day: Day::of(at).to_string(),
            name: name.to_owned(),
            path: Some(path.to_owned()),
            host: Some(self.host.to_owned()),
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
            user_id: None,
            props: None,
            source: "web",
        }
    }
}

/// How many visitors a day gets: a weekday dip, slow growth, and a launch
/// spike six days ago.
fn visitors_on(rng: &mut Rng, base: u64, age: i64) -> u64 {
    let day_index = u64::try_from(DAYS - age).unwrap_or(0);
    let weekend = if age % 7 == 2 || age % 7 == 3 { base / 3 } else { 0 };
    let spike = if age == 6 { base * 2 } else { 0 };
    base + day_index * base / 30 + spike + rng.below(base / 4) - weekend
}

/// When the `n`th visitor of a day arrives: spread over the part of the day
/// that has passed.
fn arrival(rng: &mut Rng, day: Day, now: UnixMillis) -> UnixMillis {
    let start = day.start();
    let open = u64::try_from(now.0.saturating_sub(start.0)).unwrap_or(0).min(86_399_000);
    // Keep an hour free so a visit's later steps stay in the past.
    let window = open.saturating_sub(3_600_000).max(1);
    start.plus(Duration::from_millis(rng.below(window)))
}

fn website_visit(rng: &mut Rng, site: &Site, day: Day, now: UnixMillis, age: i64) -> Visit {
    let source = if age == 6 && rng.chance(50) { SOURCES[2].1 } else { rng.pick(&SOURCES) };
    let country = rng.pick(&COUNTRIES);
    let (device, browser, os) = rng.pick(&AGENTS);
    let visitor = rng.visitor();
    let mut at = arrival(rng, day, now);
    let mut events = Vec::new();
    let mut push = |r: &mut Rng, name: &str, path: &str, props: Option<Value>| {
        let mut event = site.event(UnixMillis(at.0.min(now.0)), name, path);
        event.visitor = visitor;
        event.person = format!("v:{day}:{visitor}");
        event.country = country.map(str::to_owned);
        event.device = Some(device.to_owned());
        event.browser = Some(browser.to_owned());
        event.os = Some(os.to_owned());
        event.props = props.map(|props| props.to_string());
        if events.is_empty() {
            event.referrer_host = source.0.map(str::to_owned);
            event.utm_source = source.1.map(str::to_owned);
            event.utm_medium = source.2.map(str::to_owned);
            event.utm_campaign = source.3.map(str::to_owned);
        }
        events.push(event);
        at = at.plus(Duration::from_secs(20 + r.below(160)));
    };

    let landing = rng.pick(&LANDINGS);
    push(rng, "pageview", landing, None);
    let mut on_pricing = landing == "/pricing";
    if !on_pricing && rng.chance(if landing == "/" { 40 } else { 20 }) {
        push(rng, "pageview", "/pricing", None);
        on_pricing = true;
    }
    if rng.chance(30) {
        let extra = rng.pick(&[(1, "/features"), (1, "/about"), (1, "/changelog")]);
        push(rng, "pageview", extra, None);
    }
    let mut signed_up = None;
    if on_pricing && rng.chance(35) {
        let plan = if rng.chance(30) { "pro" } else { "free" };
        push(rng, "pageview", "/signup", None);
        push(rng, "signup_started", "/signup", Some(json!({ "plan": plan })));
        if rng.chance(55) {
            push(rng, "signup_completed", "/signup", Some(json!({ "plan": plan })));
            let (name, handle) = rng.pick(&NAMES);
            let user_id = format!("user_{}", 1000 + rng.below(9000));
            let traits = json!({
                "email": format!("{handle}.{}@example.com", rng.below(100)),
                "name": name,
                "plan": plan,
            });
            signed_up = Some((user_id, traits));
            onboard(rng, &mut push);
        }
    }
    Visit { events, signed_up, agent: (device, browser, os), country, age }
}

/// The onboarding steps right after a signup, each with its pageview. Some
/// people stop at each step, so the Onboarding funnel has drop-off.
fn onboard(rng: &mut Rng, push: &mut impl FnMut(&mut Rng, &str, &str, Option<Value>)) {
    let steps: [(u64, &str, &str, Value); 3] = [
        (80, "project_created", "/onboarding/project", json!({ "template": "blank" })),
        (55, "invite_sent", "/onboarding/invite", json!({ "role": "member" })),
        (70, "report_viewed", "/onboarding/report", json!({ "report": "weekly" })),
    ];
    for (percent, name, path, props) in steps {
        push(rng, "pageview", path, None);
        if !rng.chance(percent) {
            return;
        }
        push(rng, name, path, Some(props));
    }
    push(rng, "pageview", "/app", None);
}

/// An identified person's events on a day they come back to the app. The
/// actions are not onboarding events, so returns don't move people through
/// the Onboarding funnel.
fn return_visit(
    rng: &mut Rng,
    site: &Site,
    day: Day,
    now: UnixMillis,
    person: &Person,
) -> Vec<EventRow> {
    let mut at = arrival(rng, day, now);
    let visitor = rng.visitor();
    let mut out = Vec::new();
    let actions: [(u64, (&str, &str, &str)); 4] = [
        (4, ("project_opened", "/app/projects", r#"{"template":"blank"}"#)),
        (3, ("report_exported", "/app/reports", r#"{"report":"weekly"}"#)),
        (2, ("comment_added", "/app/team", r#"{"thread":"launch"}"#)),
        (1, ("invoice_paid", "/app/billing", r#"{"amount":49,"plan":"pro"}"#)),
    ];
    let mut add = |r: &mut Rng, name: &str, path: &str, props: Option<&str>| {
        let mut event = site.event(UnixMillis(at.0.min(now.0)), name, path);
        event.visitor = visitor;
        event.person = format!("p:{}", person.id);
        event.anon_id = Some(person.anon_id.clone());
        event.user_id = Some(person.user_id.clone());
        event.device = Some(person.agent.0.to_owned());
        event.browser = Some(person.agent.1.to_owned());
        event.os = Some(person.agent.2.to_owned());
        event.country = person.country.map(str::to_owned);
        event.props = props.map(str::to_owned);
        if name == "invoice_paid" {
            event.source = "server";
            event.device = None;
            event.browser = None;
            event.os = None;
            event.anon_id = None;
        }
        out.push(event);
        at = at.plus(Duration::from_secs(30 + r.below(600)));
    };
    add(rng, "pageview", "/app", None);
    for _ in 0..=rng.below(3) {
        let (name, path, props) = rng.pick(&actions);
        add(rng, "pageview", path, None);
        add(rng, name, path, Some(props));
    }
    out
}

/// A person the seed identified.
struct Person {
    id: PersonId,
    user_id: String,
    anon_id: String,
    agent: Agent,
    country: Option<&'static str>,
    signup_age: i64,
}

fn docs_visit(rng: &mut Rng, site: &Site, day: Day, now: UnixMillis) -> Vec<EventRow> {
    let source = rng.pick(&[
        (4, None),
        (4, Some("google.com")),
        (2, Some("github.com")),
        (1, Some("example.com")),
    ]);
    let country = rng.pick(&COUNTRIES);
    let (device, browser, os) = rng.pick(&AGENTS);
    let visitor = rng.visitor();
    let mut at = arrival(rng, day, now);
    let mut out = Vec::new();
    for i in 0..=rng.below(3) {
        let mut event = site.event(UnixMillis(at.0.min(now.0)), "pageview", rng.pick(&DOCS_PAGES));
        event.visitor = visitor;
        event.person = format!("v:{day}:{visitor}");
        event.country = country.map(str::to_owned);
        event.device = Some(device.to_owned());
        event.browser = Some(browser.to_owned());
        event.os = Some(os.to_owned());
        if i == 0 {
            event.referrer_host = source.map(str::to_owned);
        }
        out.push(event);
        at = at.plus(Duration::from_secs(40 + rng.below(300)));
    }
    out
}

fn site_row(seed: &DemoSeed, project_id: ProjectId, hostnames: &Value, mode: &str) -> SiteRow {
    SiteRow {
        project_id,
        workspace_id: seed.workspace_id,
        site_key: bizkit_core::auth::random_token()
            .chars()
            .take(crate::ops::SITE_KEY_LEN)
            .collect(),
        hostnames: hostnames.to_string(),
        identity_mode: mode.to_owned(),
        last_event_day: Some(Day::of(seed.now).to_string()),
        rolled_through: None,
        created_at: seed.now,
        updated_at: seed.now,
    }
}

fn goal_row(
    seed: &DemoSeed,
    project_id: ProjectId,
    name: &str,
    kind: &str,
    matches: &str,
) -> GoalRow {
    GoalRow {
        id: GoalId::new(),
        workspace_id: seed.workspace_id,
        project_id,
        name: name.to_owned(),
        kind: kind.to_owned(),
        matches: matches.to_owned(),
        created_at: seed.now,
    }
}

/// Seeds analytics for the `website` project (identity after identify,
/// with goals, a funnel and people) and the `docs` project (cookieless,
/// pageviews only), and records the Signup goal in `seed.entities`.
///
/// Each project has two idempotent steps: the app rows (site, goals,
/// funnel) when it has no site, then the file rows (events, people,
/// conversions) when its file has no events. App rows go first so a file
/// only exists for a project with a site; a site that outlived its events
/// (the move to project files dropped them) gets them back.
pub(crate) async fn seed(db: &Db, seed: &mut DemoSeed) -> anyhow::Result<()> {
    if let Some(&project_id) = seed.project_ids.get("website") {
        if store::find_site(db.read(), seed.workspace_id, project_id).await?.is_none() {
            website_app_rows(db, seed, project_id).await.context("seeding the website site")?;
        }
        ensure_demo_site(db, seed, project_id).await.context("adding the demo site")?;
        let goals = store::list_goals(db.read(), seed.workspace_id, project_id).await?;
        let file = open_empty(seed, project_id).await?;
        if let Some(file) = &file {
            website_events(file, seed, project_id, &goals)
                .await
                .context("seeding website events")?;
            mark_events(db, seed, project_id).await?;
        }
        if let Some(goal) = goals.into_iter().find(|goal| goal.name == SIGNUP_GOAL) {
            seed.entities.insert((GOAL.id.to_owned(), SIGNUP_GOAL.to_owned()), goal.id.to_string());
        }
        close(file).await;
    }
    if let Some(&project_id) = seed.project_ids.get("docs") {
        if store::find_site(db.read(), seed.workspace_id, project_id).await?.is_none() {
            let mut tx = db.write().await?;
            let row = site_row(seed, project_id, &json!([]), "cookieless");
            store::insert_site(&mut *tx, &row).await.context("seeding the docs site")?;
            tx.commit().await?;
        }
        let file = open_empty(seed, project_id).await?;
        if let Some(file) = &file {
            docs_events(file, seed, project_id).await.context("seeding docs events")?;
            mark_events(db, seed, project_id).await?;
        }
        close(file).await;
    }
    Ok(())
}

/// What the demo site needs on the website project, for stacks seeded
/// before it existed too: `localhost` among the hostnames and the
/// Onboarding funnel. App rows only; onboarding events come with a fresh
/// project file.
async fn ensure_demo_site(db: &Db, seed: &DemoSeed, project_id: ProjectId) -> anyhow::Result<()> {
    let mut tx = db.write().await?;
    let Some(mut site) = store::find_site(&mut *tx, seed.workspace_id, project_id).await? else {
        return Ok(());
    };
    let mut hosts: Vec<String> =
        serde_json::from_str(&site.hostnames).context("reading the website hostnames")?;
    // An empty list accepts every host, the demo site's too.
    if !hosts.is_empty() && !hosts.iter().any(|host| host == DEMO_SITE_HOST) {
        hosts.push(DEMO_SITE_HOST.to_owned());
        site.hostnames = json!(hosts).to_string();
        site.updated_at = seed.now;
        store::update_site_settings(&mut *tx, &site).await?;
    }
    let has_funnel = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM analytics_funnels
                         WHERE workspace_id = ? AND project_id = ? AND name = ?) AS "has!: bool""#,
        seed.workspace_id,
        project_id,
        ONBOARDING_FUNNEL,
    )
    .fetch_one(&mut *tx)
    .await?;
    if !has_funnel {
        let funnel = FunnelRow {
            id: FunnelId::new(),
            workspace_id: seed.workspace_id,
            project_id,
            name: ONBOARDING_FUNNEL.to_owned(),
            steps: json!([
                { "kind": "event", "match": "signup_completed" },
                { "kind": "event", "match": "project_created" },
                { "kind": "event", "match": "invite_sent" },
                { "kind": "event", "match": "report_viewed" },
            ])
            .to_string(),
            window_days: 7,
            created_at: seed.now,
            updated_at: seed.now,
        };
        funnels::save(&mut *tx, &funnel).await?;
    }
    tx.commit().await
}

/// The site key of the demo workspace's website project, for the demo
/// site's snippet tag. `None` before `seed-demo` has run.
///
/// # Errors
///
/// When the query fails.
pub async fn demo_site_key(db: &Db) -> anyhow::Result<Option<String>> {
    let key = sqlx::query_scalar!(
        "SELECT s.site_key FROM analytics_sites s
         JOIN projects p ON p.id = s.project_id
         JOIN workspaces w ON w.id = s.workspace_id
         WHERE w.slug = ? AND p.slug = 'website'",
        bizkit_core::dev::demo::DEMO_WORKSPACE,
    )
    .fetch_optional(db.read())
    .await
    .context("finding the demo site")?;
    Ok(key)
}

/// Sets the site's event day to today. A site that survived the move to
/// project files had it reset, and reports treat it as having no events.
async fn mark_events(db: &Db, seed: &DemoSeed, project_id: ProjectId) -> anyhow::Result<()> {
    let mut tx = db.write().await?;
    let today = Day::of(seed.now).to_string();
    store::advance_event_day(&mut *tx, seed.workspace_id, project_id, &today)
        .await
        .context("marking the seeded site's event day")?;
    // A rollup marker from an earlier file would hide the refilled days.
    sqlx::query!(
        "UPDATE analytics_sites SET rolled_through = NULL WHERE workspace_id = ? AND project_id = ?",
        seed.workspace_id,
        project_id,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

/// The project's file when it has no events yet.
async fn open_empty(seed: &DemoSeed, project_id: ProjectId) -> anyhow::Result<Option<ProjectDb>> {
    let file = ProjectDb::open(&project_db_path(&seed.data_dir, project_id)).await?;
    let events: i64 = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "n!: i64" FROM analytics_events WHERE workspace_id = ?"#,
        seed.workspace_id,
    )
    .fetch_one(file.read())
    .await?;
    if events > 0 {
        file.close().await;
        return Ok(None);
    }
    Ok(Some(file))
}

async fn close(file: Option<ProjectDb>) {
    if let Some(file) = file {
        file.close().await;
    }
}

async fn website_app_rows(db: &Db, seed: &DemoSeed, project_id: ProjectId) -> anyhow::Result<()> {
    let signup = goal_row(seed, project_id, SIGNUP_GOAL, "event", "signup_completed");
    let pricing = goal_row(seed, project_id, "Viewed pricing", "page", "/pricing");
    let funnel = FunnelRow {
        id: FunnelId::new(),
        workspace_id: seed.workspace_id,
        project_id,
        name: "Signup".to_owned(),
        steps: json!([
            { "kind": "page", "match": "/" },
            { "kind": "page", "match": "/pricing" },
            { "kind": "event", "match": "signup_started" },
            { "kind": "event", "match": "signup_completed" },
        ])
        .to_string(),
        window_days: 7,
        created_at: seed.now,
        updated_at: seed.now,
    };
    let mut tx = db.write().await?;
    let hostnames = json!(["example.com", "www.example.com"]);
    store::insert_site(&mut *tx, &site_row(seed, project_id, &hostnames, "after_identify")).await?;
    store::insert_goal(&mut *tx, &signup).await?;
    store::insert_goal(&mut *tx, &pricing).await?;
    funnels::save(&mut *tx, &funnel).await?;
    tx.commit().await
}

async fn website_events(
    file: &ProjectDb,
    seed: &DemoSeed,
    project_id: ProjectId,
    goals: &[GoalRow],
) -> anyhow::Result<()> {
    let site = Site { workspace_id: seed.workspace_id, project_id, host: "example.com" };
    let today = Day::of(seed.now);
    let mut rng = Rng(0x5eed_0001);
    let mut tx = file.write().await?;
    let mut people: Vec<Person> = Vec::new();
    for age in (0..DAYS).rev() {
        let day = today.plus_days(-age);
        let mut events = Vec::new();
        for _ in 0..visitors_on(&mut rng, 70, age) {
            let mut visit = website_visit(&mut rng, &site, day, seed.now, age);
            if let Some(person) = identify(&mut tx, seed, project_id, &mut visit, &mut rng).await? {
                people.push(person);
            }
            let visit_events = visit.events;
            events.extend(visit_events);
        }
        for person in people.iter().filter(|person| person.signup_age > age) {
            let weeks = (person.signup_age - age) / 7;
            if rng.chance(if weeks == 0 { 12 } else { 6 }) {
                let back = return_visit(&mut rng, &site, day, seed.now, person);
                if let Some(at) = back.last().map(|event| event.ts) {
                    store::upsert_person(
                        &mut *tx,
                        seed.workspace_id,
                        project_id,
                        &person.user_id,
                        at,
                    )
                    .await?;
                }
                events.extend(back);
            }
        }
        events.sort_by_key(|event| event.ts);
        for event in &events {
            store::insert_event(&mut *tx, event).await?;
            for goal in goals {
                if crate::write::converts(goal, event) {
                    store::insert_conversion(
                        &mut *tx,
                        event.workspace_id,
                        goal.id,
                        &event.day,
                        &event.person,
                    )
                    .await?;
                }
            }
        }
    }
    tx.commit().await.context("committing website events")?;
    Ok(())
}

/// Stores the person a signup visit made, and moves the visit's events to
/// them, as identify's backfill would.
async fn identify(
    tx: &mut SqliteConnection,
    seed: &DemoSeed,
    project_id: ProjectId,
    visit: &mut Visit,
    rng: &mut Rng,
) -> anyhow::Result<Option<Person>> {
    let Some((user_id, traits)) = visit.signed_up.take() else { return Ok(None) };
    let Some(at) =
        visit.events.iter().find(|event| event.name == "signup_completed").map(|event| event.ts)
    else {
        return Ok(None);
    };
    let id = store::upsert_person(&mut *tx, seed.workspace_id, project_id, &user_id, at).await?;
    store::set_person_traits(&mut *tx, seed.workspace_id, id, &traits.to_string()).await?;
    let anon_id = format!("anon-{:x}", rng.next());
    store::set_alias(&mut *tx, seed.workspace_id, project_id, &anon_id, id).await?;
    for event in &mut visit.events {
        event.person = format!("p:{id}");
    }
    if let Some(signup) = visit.events.iter_mut().find(|event| event.name == "signup_completed") {
        signup.user_id = Some(user_id.clone());
        signup.anon_id = Some(anon_id.clone());
    }
    Ok(Some(Person {
        id,
        user_id,
        anon_id,
        agent: visit.agent,
        country: visit.country,
        signup_age: visit.age,
    }))
}

async fn docs_events(
    file: &ProjectDb,
    seed: &DemoSeed,
    project_id: ProjectId,
) -> anyhow::Result<()> {
    let site = Site { workspace_id: seed.workspace_id, project_id, host: "docs.example.com" };
    let today = Day::of(seed.now);
    let mut rng = Rng(0x5eed_0002);
    let mut by_day: BTreeMap<String, Vec<EventRow>> = BTreeMap::new();
    for age in (0..DAYS).rev() {
        let day = today.plus_days(-age);
        for _ in 0..visitors_on(&mut rng, 30, age) {
            by_day
                .entry(day.to_string())
                .or_default()
                .extend(docs_visit(&mut rng, &site, day, seed.now));
        }
    }
    let mut tx = file.write().await?;
    for mut events in by_day.into_values() {
        events.sort_by_key(|event| event.ts);
        for event in &events {
            store::insert_event(&mut *tx, event).await?;
        }
    }
    tx.commit().await.context("committing docs events")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_generator_is_deterministic_and_weighted() {
        let mut a = Rng(1);
        let mut b = Rng(1);
        assert_eq!(a.next(), b.next());
        let mut rng = Rng(7);
        let picks: Vec<&str> = (0..1000).map(|_| rng.pick(&[(9, "a"), (1, "b")])).collect();
        let bs = picks.iter().filter(|pick| **pick == "b").count();
        assert!((50..200).contains(&bs), "{bs}");
    }

    #[test]
    fn the_launch_day_is_the_busiest() {
        let mut rng = Rng(3);
        let counts: Vec<u64> = (0..DAYS).map(|age| visitors_on(&mut rng, 70, age)).collect();
        let max = counts.iter().copied().max().unwrap_or(0);
        assert_eq!(counts[6], max, "{counts:?}");
    }
}
