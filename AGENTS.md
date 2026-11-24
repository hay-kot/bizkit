# AGENTS.md

Guidance for coding agents working in this repository.

## Before you finish

Run `mise run check`. It is what CI runs, and it must pass: formatting, clippy with warnings as errors (with and without the `test-hooks` feature), tests, docs, cargo-deny, cargo-shear, typos, the sqlx query data (`db:check`), the generated API client (`gen:check`) and the web checks (`web:check`). CI also runs `mise run test:e2e`.

A PR that changes the UI attaches screenshots to its body, and a recording when the change is a multi-step flow. The `pr-assets` skill (`.claude/skills/pr-assets`) uploads them with `pra` and has a sample `pr-record` scene.

## Layout

- `crates/bizkit`: the binary. `main.rs` owns process concerns; `modules.rs` lists the compiled-in feature modules and holds the tests that sweep every operation.
- `crates/bizkit-core`: the platform: config, database, operations, auth, tenancy, jobs and the `Module` trait.
- `crates/bizkit-web`: embeds `web/dist` and serves it as the fallback route.
- `crates/bizkit-testkit`: fakes, `TestApp`, the spec-checking `SpecClient` and `McpClient`. Integration tests live in its `tests/`.
- `web/`: the Vue SPA. `e2e/`: Playwright (see `e2e/AGENTS.md`).

## Lints

The lint set in `Cargo.toml` is strict on purpose. Fix the code rather than silencing the lint. When a lint is wrong for one spot, silence it there with a reason:

```rust
#[expect(clippy::cast_possible_truncation, reason = "value is checked to fit in u8 above")]
```

Never use `#[allow(...)]` (the `allow_attributes` lint rejects it), and never add a crate-level or `Cargo.toml` allow without asking.

## Conventions

- `main.rs` owns process concerns: parsing, logging setup, error reporting and the exit code. Commands never call `std::process::exit`.
- Commands write output to the `out: &mut impl Write` they are given, never with `println!`. Logs go to stderr through `tracing`.
- Library-style modules return `anyhow::Result` with context on every fallible call (`.with_context(|| ...)`). Use `thiserror` if a caller needs to match on error variants.
- No `unwrap`, `expect` or `panic!` outside tests.
- Add a command with `scaffold new command`, not by hand, so all registration points stay in sync.

## Operations

An API operation is one `Operation` impl: ID, method, path under `/api/v1`, access, input, output and summary. The registry turns it into a JSON route, an OpenAPI entry and, for `Access::Workspace` operations, an MCP tool named after the ID (`projects.create` → `projects_create`).

- The input is one struct merged from path params, query params and the JSON body. Path params are the fields named like the `{...}` segments. Derive `Deserialize` and `ToSchema`, use `#[serde(deny_unknown_fields)]`, and implement `Validate`.
- List outputs are objects with `items`, never bare arrays, so they can grow pagination fields.
- Return `ApiError` variants. Document statuses beyond the access defaults (such as 409) in `ERRORS`; the spec-checking client fails tests on undocumented statuses.
- Writes go through `cx.write()`, a `BEGIN IMMEDIATE` transaction. Never call `write()` while you hold one.
- Mail and other side effects go through jobs (`platform.bus().enqueue(&mut tx, ...)`) in the same transaction.

## Modules

A feature module implements `bizkit_core::Module` in its own `crates/bizkit-<name>` crate and is added to `crates/bizkit/src/modules.rs`.

- `register` declares scope areas (named after the module), operations (paths under `/workspaces/{workspace_id}/<name>` or `/<name>`), raw routes (nested at `/m/<name>`), jobs (kinds starting with `<name>.`) and subscribers. It must not do I/O or branch on config.
- Endpoints that third-party pages call (form ingest) go in `public_routes`, nested at `/m/<name>/public`. They sit outside authentication: no `Authn`, no cookies, no CSRF check. Escape anything they echo into HTML.
- `seed_demo` adds the module's sample rows to the dev stack's demo workspace (`bizkit dev seed-demo`, run by the svx `seed` service). Keep it idempotent, put it behind the crate's own `test-hooks` feature, and extend it whenever the module gains a feature.
- Module tables carry `workspace_id`, and every query filters on it. The exceptions are lookups by an unguessable key that arrives before bizkit knows the workspace (a public link's token, a provider webhook's delivery or message ID), by the contact ID of a signed manage link, which counts only after its MAC checks out, and the startup sweep that seals plaintext extension secrets across workspaces: they read the workspace from the row they find, and the replay table for webhook tokens holds no tenant data, so it has no `workspace_id`.
- Module config lives under `modules.<name>` in the config file, parsed into the module's own struct with `deny_unknown_fields`. Analytics reads `geoip_path` (a MaxMind-format country file; without it countries are unknown) `raw_retention_days` (default 90, at least 7) `flush_ms` (the write window, default 100, 10 to 250) and `self_usage` (below).
- Public routes that need the visitor's address take the `ClientIp` extractor. It reads the peer address, or, when the top-level `trusted_proxies` is n, the nth `X-Forwarded-For` entry from the right. Set it to the number of proxies in front of bizkit that append the header (the dev stack has one, Vite); a higher number lets clients claim any address.
- Analytics counts by UTC day. Ingest (the snippet, the beacon and server events) goes through `Files::submit`, the one write path: it queues events for the project's buffered writer, which commits them every `flush_ms` or at 1,000 queued. A crash loses at most one window; a clean shutdown loses none. Reports read `analytics_daily` up to `rolled_through` and raw events after it, so a report gives the same numbers before and after the rollup job.
- Analytics keeps events, rollups, people, aliases and conversions in one SQLite file per project (`data_dir/analytics/<project_id>.db`, migrations in `migrations-project/`). Sites, goals, funnels and salts stay in the app database. A query must not join app tables with file tables: read the app rows first, then the file. Deleting a project publishes `ProjectDeleted`, and analytics removes the file.
- A module with buffered work registers a `Lifecycle` (`cx.lifecycle`): `flush` runs on `Platform::flush_modules` and before jobs drain, `stop` on shutdown.
- Analytics tests build with `Analytics::manual_flush()` (no timer), call `app.flush()` after ingest and before every read, and open a project's file with `bizkit_analytics::testing::project_pool(app.data_dir(), project_id)`. Only tests of the writer's own timing build with the timer (`Analytics::default()`) and poll the file.
- Analytics insights are one typed query (`analytics.insights.query`, POST; `crates/bizkit-analytics/src/insights/`): kind `events` or `people`, filters (`is`, `is_not`, `gt`, `lt`, `is_set`), a measure (`total`, `users`, `share`, `per_user`) and a breakdown by one property or trait, answered as a result tagged by `view`. There is no SQL surface: every query is static SQL with the filters as one JSON array, so nothing in a query object reaches SQL text. `users` counts every person key; `share` and `per_user` leave out cookieless `v:` keys, as retention does. Events insights read raw events only, so they reach back as far as raw retention (`raw_since` says when a range starts earlier); lifetime counts belong in traits and the `people` kind, which ignores the range. Saved insights (name, query, range preset) and dashboards (ordered tiles in `analytics_dashboard_tiles`, each an insight and a `width` of 4, 6, 8 or 12 columns of a 12-column grid, at most 20) are app-database config like funnels; the API takes and returns the whole `tiles` list, and a row is as tall as its tallest tile, so no height is stored. Deleting an insight takes it off every dashboard by cascade.
- Analytics schema lookups (`analytics.schema.events`, `.keys`, `.values`; `crates/bizkit-analytics/src/schema.rs`) list what a project sent, for pickers and for agents writing insight queries: event names (pageview included) and the keys and top values of `props` (source `property`) or `traits` (source `trait`). Each reads at most 50,000 rows, newest first, through an index, applies the name, key and `q` filters to those rows, and says `sampled` when the read reached the cap. Properties read raw events in the range (default the last 30 days); traits read every person and ignore the range. Keys and `q` are bound parameters, keys match `json_each`'s `key` column and never a JSON path, and `q` goes through the escaped `LIKE` pattern. The web reads them through `useSchemaEvents`, `useSchemaKeys` and `useSchemaValues` (60 s stale time). The insight editor (`components/analytics/InsightEditor.vue`, shared by the new and saved insight pages) has two views of the one query: the Builder, whose pickers read these lookups, and Code, the `InsightQuery` JSON in `JsonEditor`. In Code the JSON is the source of truth and runs on Run or Cmd/Ctrl+Enter; switching back to the Builder parses it (`insightCode.ts`), and JSON the Builder can't show exactly (unknown fields, more than 10 filters, a value it would change) keeps the editor on Code with a message. A saved query the Builder can't show opens in Code.
- bizkit reports its own usage through the core `UsageReported` event (`bizkit_core::usage`): core ops publish it in the action's transaction, for user principals (an API-key call reports no event, only fresh traits for the users it affected), and analytics sends it to the project `modules.analytics.self_usage` names by slugs (`{workspace, project}`; the project needs a site). Each report is a server event at the action's time (now, if the delivery runs on a later UTC day) plus an `$identify` with the user's current `workspaces` and `api_keys` counts; `affected` users (a revoked key's creator, a removed member) get fresh traits too. What goes out is the `UsageKind` list, user IDs and workspace IDs: no emails, names or slugs, and members of the target workspace see it for every user of the instance. Unset, the subscriber still runs (register doesn't branch on config) and sends nothing. A new kind goes in `UsageKind`, reported with `usage::report(&cx, &mut tx, Report::new(..))`. The dev stack reports into the demo workspace's `bizkit` project; its seed fills while the file has no `seed_user_` people.
- Modules share collected addresses through the core `ContactCaptured` event (`bizkit_core::contacts`), not a crate dependency: forms publishes it in the submission's transaction when a submission has an email, and mailing subscribes. Another module that collects an address with the person's action publishes the same event.
- Mailing keeps consent per list: a contact row belongs to one list. Form captures always confirm by mail, whatever the list's `double_opt_in`; API and admin adds follow it. Unsubscribed is final: nothing re-subscribes or mails that row, and deleting it is refused, because it is the suppression record. Confirm mails are capped per list per hour.
- Mailing's public confirm and unsubscribe pages change nothing on GET (mail scanners follow links); a button POSTs. `POST /unsubscribe` with `List-Unsubscribe=One-Click` answers RFC 8058 one-click, the URL `Mail.unsubscribe` puts in the `List-Unsubscribe` header.
- Every mail mailing sends is a `mailing_deliveries` row and one `mailing.send` job, which renders at send time, skips contacts that left the list, and records the result (the send log). A broadcast fans out through `mailing.broadcast` in batches of 500 that resume after a crash. Sending is at least once. `mailing.broadcasts.resend` puts only `failed` deliveries back in the queue and counts it in `resends`, so a resend never mails anyone twice.
- Mail transport is core and set per instance: `mail.provider` is `smtp` (through `smtp_url`, or the log without it) or `mailgun` (the `mail.mailgun` section, key from `api_key_env`); `mail.rate_per_second` caps each mailer (the instance one and each workspace account) separately. `Mailer::send` returns the provider's message ID or a `MailError`: `Permanent` (SMTP 5xx except 530/535, Mailgun 400/413) fails a delivery at once, `Transient` retries. A job that must not retry marks its row and returns `Ok`, because the worker retries any `Err`.
- Workspace mail: a workspace may bring its own SMTP or Mailgun account and sender (`mail.settings.get/put/delete/test`, admin; `crates/bizkit-core/src/mail/`). Send every mail through `platform.mail().send` (the `mail.send` job does), never `deps().mailer`, and set `Mail.workspace_id` on workspace mail (mailing, forms notifications, signing, extensions `ctx.mail.send`); sign-in and invitations stay `None` and use the instance mailer. The router builds a workspace's mailer on demand, cached by `updated_at`; settings that can't be used fail transient and never fall back to the instance identity. Credentials are sealed with the instance key (`secrets_key_env`, 32 bytes base64; `bizkit_core::secrets`), bound to the row by associated data, and never come back from the API. Workspace SMTP hosts must be public (`net::check_host`, pinned at connect); development builds (`test-hooks`) also accept loopback so Mailpit works. Mailgun events for a workspace account arrive at `/m/mailing/public/events/mailgun/{workspace_id}`, checked with that workspace's signing key, and touch only its deliveries.
- Mailing suppressions are workspace-wide (`mailing_suppressions`): every send but a test send skips a suppressed address. Mailgun's event webhook (`/m/mailing/public/events/mailgun`, key from `mail.webhook_signing_key_env`) records bounces and complaints on the delivery (`event`, beside `status`) and suppresses the address; a complaint also unsubscribes every contact row with it and can't be removed. A broadcast pauses (`paused_at`) after 20 deliveries fail in a row; its later sends park (`parked_at`) and Resume queues only parked rows. `send_at` schedules a draft through `mailing.broadcast.start`. The API derives `scheduled` and `paused`: never widen the broadcast status CHECK, since rebuilding `mailing_broadcasts` nulls `mailing_deliveries.broadcast_id`.
- Mailing privacy: broadcast and step mails carry a "Manage subscriptions" link (`/m/mailing/public/manage`), signed with the workspace's key in `mailing_link_keys` (never with a token a mail prints) and valid for 90 days. The page covers the address's lists in one project and only turns mail off: Save unsubscribes the lists it showed that the reader unticked. `mailing.addresses.export` and `mailing.addresses.erase` (admin, address in the body so it stays out of logs) cover the workspace. Erase deletes the address's deliveries, contacts and suppression and keeps only a hash in `mailing_erasures`; sends, adds, suppressions and form captures refuse an erased address, for good. Keep addresses out of job errors: core's `jobs` table is out of erase's reach, so provider error text stays on the delivery row. Erase also rewrites a broadcast `pause_reason` that quotes the address. A send job past its suppression check when an erase commits still sends that one mail.
- Mailing sequences are ordered steps with a delay each; an enrollment is one contact's run. `on_join` sequences enroll a contact when it becomes subscribed (an add without double opt-in, or a confirm), only while `enabled`; `manual` ones only through `mailing.enrollments.create`. The `mailing.advance` job sends one step per run and schedules the next with `run_at`. The next step is always the lowest-position step with no delivery for the enrollment, so step edits keep IDs, never resend a step and never strand a run. Unsubscribe ends active enrollments. The `mailing.step_condition` sync hook (target kind `mailing.sequence`) can skip a step and fails open. The demo seed adds no active enrollments, because a seed can't queue the advance job.
- Signing (`crates/bizkit-signing`) is out of the product until signers can place fields on the pages: the `bizkit` crate's `signing` feature is off by default, so default builds have no signing routes, operations or MCP tools, and the web module was removed. CI still builds and tests it through `--all-features`; its tables stay in existing databases. A request is a PDF (at most 700 KiB, base64 through the API, stored in `signing_files` so it cascades) and 1 to 10 signers. `send` mails each signer a private link (only the token's hash is stored; at most 100 signer mails per workspace per hour). Signers use the public pages at `/m/signing/public` (GETs change nothing; the page's script records one `viewed`; the POST records consent and a drawn or typed signature with IP and user agent). The last signature queues `signing.complete` in its own `signing` lane: it appends a signatures page and an audit page (standard-14 fonts, WinAnsi, other characters print as `?`) and seals the file with a PAdES B-B signature (`seal.rs`: prepare-then-patch `/ByteRange` and `/Contents`, CMS with ESS signing-certificate-v2 and no signing-time) from the instance key in `signing_seal`, which has no `workspace_id` because it is one per instance. A seal that fails on its last attempt marks the request `failed`; `signing.requests.retry` queues it again. Void is refused once everyone signed. Build PDF text from content operators with hex strings, never by formatting strings.
- Migrations live in the module crate's `migrations/` and return from `Module::migrations` as `sqlx::migrate!()`. Versions must be unique across all crates; a test checks it.
- `Module::startup` runs once after migrations and before serving, for idempotent data upgrades that need runtime state a SQL migration can't see (the secrets key).

## Hooks

Modules let workspace scripts run at **hook points** they declare. Core owns the contract (`bizkit_core::hooks`); the `extensions` module runs the scripts in QuickJS sandboxes. A module never depends on extensions, and without it every point is inert.

- Declare points in `register`. IDs start with the module name (`forms.pre_validate`). Keep the `HookPoint` consts in the module's `hooks.rs` with a `sample` input.
- **Target kinds.** When scripts should pick which of the module's entities they run for (form A and C, not B), declare a `TargetKind` (`forms.form`, owned like a point ID) with `cx.hook_target_kind(&KIND, resolver)` before the points, and set `target: Some(&KIND)` on them. The `TargetResolver` finds entities by ID (returning canonical IDs) and lists a project's entities for the picker. Every hook call then passes the entity in `HookTarget.entity`.
- A script's target set is either empty (every entity of the kind) or a list of entities. Async points run every matching script. Sync points run one script per entity: one that names the entity wins over the one for all, and the API refuses (409) a second script naming the same entity on a sync point, or a second one for all.
- **Sync** (`cx.sync_hook`): runs inside the request through `platform.call_sync_hook(&POINT, target, &input).await`, with a budget of at most 100 ms and an `output` check on the returned value. Sync hooks fail open: on any failure the call gives `HookResult::Continue`, so a broken script never blocks the request. Scripts get the globals and `ctx.log` only. One script per entity and point (see target kinds above).
- **Async** (`cx.async_hook::<Event, _>(POINT, source)`): runs as a retried job for each event (3 attempts; a failed attempt reruns the whole script, so scripts must tolerate repeats). The `HookSource` gives the target project and rebuilds the input when the run starts, so job payloads hold IDs only and a deleted row skips the run. Scripts also get `ctx.secrets`, `ctx.fetch` (public addresses only) and `ctx.mail.send`.
- Inputs are plain JSON with no secrets or internal keys. Leave control fields out. Cap any text you take back from a script.
- Tests use `FakeHooks` with `TestAppBuilder::hook_runner` to script outcomes per point. Test that every `RunStatus` failure leaves the request working.
- **Script API.** `crates/bizkit-extensions/src/reference.rs` is the one source for what scripts can call: signature, limits, errors and an example per entry. `extensions.hooks.list` (MCP `extensions_hooks_list`) returns it per point as `api`, and the script editor shows it. When you change `sandbox/prelude.js`, update the table; a sandbox test checks that every entry exists in a run (`ctx.mail.send` through the mail integration test). All helpers take and return strings: there are no typed arrays, `TextEncoder`, `crypto.subtle`, `setTimeout` or `import`.

  | Name | Points | Notes |
  |---|---|---|
  | `crypto.digest`, `crypto.hmac` | all | SHA-256 or SHA-1, hex or base64 out, data at most 1 MiB |
  | `crypto.timingSafeEqual`, `crypto.randomUUID` | all | compare signatures with `timingSafeEqual`, never `===` |
  | `encoding.base64Encode/Decode`, `encoding.hexEncode/Decode` | all | UTF-8 text in and out; decoding non-UTF-8 bytes throws |
  | `URL`, `URLSearchParams` | all | WHATWG; `href`, `pathname`, `search`, `hash` are settable |
  | `ctx.log`, `console.*` | all | 50 lines of 500 characters |
  | `ctx.secrets` | async | the project's secrets, redacted from run records |
  | `ctx.fetch(url, {method, headers, body \| json \| form, timeoutMs})` | async | 5 calls per run, 5 s each, 256 KiB each way; non-2xx resolves |
  | `ctx.mail.send({to, subject, text, html?})` | async | 3 per run; 50 per script and 200 per workspace per hour |

- Scripts keep saved test cases (`[{name, input, expect?}]`); give each seeded script cases that pass against its seeded source.
- **Entity sections.** A web module adds panels to other modules' entity pages through `entitySections` in `defineWebModule`; the page renders `<EntitySections kind="forms.form" :entity-id ...>` and never imports the module. Extensions uses it to list the scripts that run for the entity, so a new target kind gets that panel by rendering `EntitySections` on its page.
- Outbound HTTP goes through `Deps.http`, which reaches public addresses only. Check user URLs with `bizkit_core::net::check_url` too, for a clear error.
- Project secrets (`extension_secrets`) are sealed with the instance key (`crates/bizkit-extensions/src/secrets.rs`, associated data naming workspace, project and name). Without a key, `extensions.secrets.put` answers 409 and rows from before sealing stay plaintext and keep working; startup seals them once a key is set, and drops the plaintext, so keep the key it started with. Reading a sealed secret that doesn't open from `ctx.secrets` throws an error that names it, rather than giving `undefined`; scripts that don't read it run as usual.

## Database and generated code

- Queries use the `sqlx::query!` macros. Builds read the committed `.sqlx/` (`SQLX_OFFLINE=true` in `mise.toml`). After changing a query or a migration, run `mise run db:prepare` and commit `.sqlx/`.
- After changing an operation or a schema type, run `mise run gen`. It rewrites `openapi.json` and `web/src/api/gen`; commit both.

## Frontend

- Vue 3, Tailwind v4 and DaisyUI 5. Use Tailwind and DaisyUI classes only: ESLint rejects unknown classes, arbitrary values (`w-[13px]`), raw palette colors (`bg-blue-500`) and inline styles. Use DaisyUI semantic colors (`primary`, `base-200`, `error`).
- Colors are defined only in `web/src/styles/theme.css`; stylelint rejects hex, named and functional colors elsewhere. Surfaces step up canvas (`base-200`: page and sidebar) < panel (`base-100`: cards, tables) < `raised` (menus, popovers); inputs use `field` and `field-border`. `primary` is the orange accent (buttons, links, active tab and nav, focus, first chart series), and charts color series with `chart-1` to `chart-4`.
- Only `web/src/queries/**` and `web/src/api/client.ts` import the generated client in `web/src/api/gen`. Pages use the composables in `src/queries`.
- Disabling an ESLint rule needs a reason: `// eslint-disable-next-line rule -- why`. The only inline styles are the floating panels in `Select.vue` and `TimeRangePicker.vue`, which need coordinates. Each wraps its panel in a block disable with a reason: `<!-- eslint-disable vue/no-restricted-v-bind -- why -->` before it and `<!-- eslint-enable vue/no-restricted-v-bind -->` after it.
- Layout: `/` is the user home (workspaces, invitations, create). Inside a workspace, `layouts/WorkspaceLayout.vue` gives the sidebar: workspace switcher on top, then the project switcher (a bordered block of its own, so it doesn't read as a nav row), search, the selected project's pages, then the workspace meta pages, then the user menu. The active page gets an accent bar. The sidebar shares the page's canvas color, and the content column is full width. `/w/:workspaceId` lands on the current project's first page (the last project used, else the first), or on Projects when there are none.
- A feature's UI is a web module: `web/src/modules/<name>.ts` calls `defineWebModule({ name, projectRoutes, workspaceRoutes, nav })` and is listed in `modules/index.ts`. The router and the sidebar read the list, so a module never edits the shell. Module data belongs to a project, so feature pages go in `projectRoutes` (mounted at `/w/:workspaceId/p/:projectId/`, params arrive as props) with nav items in `section: 'project'`; they show under the sidebar's project switcher. `section: 'workspace'` is for workspace meta (Team, API keys, Settings) pinned to the bottom. Sidebar `order` 0-99 is core's; modules start at 100; Settings stays last. A unit test checks that nav items point at registered routes of their section and nothing collides.
- **Command palette.** A module adds rows with `palette: (cx) => rows` on `defineWebModule`. `PaletteRows` in the workspace layout calls it in a component setup (so query composables work) and calls it again when the workspace, project or admin flag changes. Nav items become rows on their own. Rows name the object or action (`Contact`, `New form`), never the path: `group` is the container (`Forms`), `kind` a muted type word (`form`), and verbs go in `keywords`. Objects use `scope: 'goto'` and ids like `forms.form:<id>`; actions keep the default scope. Return `[]` when the rows need a project or admin rights the user lacks. A page can add page-local rows with `usePaletteRows(rows)`; they go away with the page. A "New ..." action links to its list page with `?new=1`, and the page calls `useCreateIntent(openPanel)`.
- Pages follow the archetypes in `web/DESIGN.md` (list, detail, settings, report): read it before you add or restyle a page. Detail and edit pages are one column.
- Building blocks, and what ESLint enforces outside `src/components/ui`:

  | Need | Use | Enforced by |
  |---|---|---|
  | A page | `Page` (full width; `compact` only for one-card pages outside the shell) > `PageHeader` (icon and breadcrumb come from the route's module nav item; pass `icon` only outside a module; `Tabs` go in its `tabs` slot) > `PageSection`s and `Card`s | `grid-cols-*`, `col-span-*`, `columns-*`, `grid-flow-col` and `max-w-*` are errors in `src/pages` |
  | A detail page's header actions | `DetailActions`: page buttons, Save (its label is the save state) and a `#menu` of `MenuItem`s (`danger` for delete, last) | Pages can't import `ActionsMenu` |
  | Rows compared by column | `DataTable` with `#cell-<key>` slots; it handles loading, error and empty. `rowTo` for rows that open a page, `@row-click` for rows that open a panel; mark columns with their own controls `interactive` | `<table>` and DaisyUI `table*` classes are errors; `link-hover` is an error outside `components/ui` |
  | Entities read one per row (Team, Projects, an inbox) | `List` with `ListItem`: `to` or `@select` makes the row the target, `#trailing` actions stay clickable; it handles loading, error and empty | `link-hover` is an error outside `components/ui` |
  | A few big things to pick from | `CardGrid` with `LinkCard`s | `grid-cols-*` is an error in `src/pages` |
  | Ordered steps with gaps between them | `Timeline` with `TimelineItem` (numbered marker, `#chip` for the gap) | |
  | A choice | `Select` | `<select>`, `input[type=radio]` and DaisyUI `select*` classes are errors |
  | A choice from a long or open list, by typing | `Combobox` (grouped options with counts, `allowCustom` for free text) | |
  | A tick box | `Checkbox` | `input[type=checkbox]` and DaisyUI `checkbox`/`toggle` classes are errors |
  | A create or edit form | `FormPanel` (side panel, or `variant="dialog"`), `Field`, `SubmitButton` | Pages can't import `Overlay`; DaisyUI `modal` classes are errors |
  | A field's width | `Field` caps its control (`lg` default, `md` for short values, `full`); pages stay full width | `max-w-*` is an error in `src/pages` |
  | A toolbar row | `Select size="toolbar"`, `input h-9` and `btn h-9`: all 36px; form controls are 40px, controls in table rows 32px (`Select size="sm"`, `input-sm`) | |
  | A menu | `Menu` and `MenuItem` | DaisyUI `dropdown` classes are errors |
  | A short form under a trigger (a filter) | `Popover` (lists inside it count as inside) | DaisyUI `dropdown` classes are errors |
  | Waiting | `SubmitButton` `pending`, `SkeletonRows`, `Spinner` | DaisyUI `loading` classes are errors |
  | Confirm, run, report | `runAction({ confirm, run, success })` | Pages can't import `lib/confirm`; `alert`/`confirm`/`prompt` are errors |
  | A date | `formatDate`, `formatShortDate`, `isPast` from `lib/format.ts` | `toLocale*String` and `new Intl.*` are errors elsewhere |
  | A number | `formatNumber`, `formatCompactNumber`, `formatPercent` from `lib/format.ts` | `toLocale*String` and `new Intl.*` are errors elsewhere |
| A UTC day (analytics buckets) | `formatUtcDay`, `formatShortUtcDay` from `lib/format.ts`; the local-time formatters shift it a day west of UTC | |
  | Tabs between a feature's pages | A parent route wrapping `TabLayout`, tabs as child routes, each rendering a `TabPage` (see `components/mailing/MailingLayout.vue`); `WorkspaceLayout` keys by `routeKey`, so the layout stays mounted across tabs | |
  | A report's date range | `TimeRangePicker` (through `RangeToolbar`): props in, one `apply` out, compare in its slot. `shiftRange`, `zoomOut`, `presetRange` and `previousPeriod` from `lib/range.ts` do the UTC day math; `useReportRange` keeps the range and compare flag in the URL | |
  | KPIs with a change | `StatTiles` | |
  | Numbers over time | `TimeSeriesChart` (SVG, no chart library) | Inline styles and arbitrary values are errors |
  | A ranked list with share bars | `BarList` (bars are `progress` elements) | Inline styles and arbitrary values are errors |
  | Report panels side by side | `ReportGrid` | `grid-cols-*` is an error in `src/pages` |
  | Tiles a user arranges (dashboards) | `DashboardGrid` with `DashboardTile`s: widths 4/6/8/12, pointer and arrow-key reorder in edit mode | `grid-cols-*` and `col-span-*` are errors in `src/pages` and `src/components/analytics` |
  | Two to five short choices side by side (Builder or Code) | `SegmentedControl` (a radio group) | `input[type=radio]` is an error |
  | API data | the composables in `src/queries` | Pages and layouts can't import `@pinia/colada` or `src/api/gen` |
  | A name with a slug | `useSlugFromName()` | |
  | A page or action reachable by keyboard | a module `palette` source, or `usePaletteRows` for page-local rows | |

- When a page needs something these don't cover, add a primitive to `src/components/ui` (or a feature component that wraps one) instead of reaching for the raw element.
- Pages load eagerly and only the content area animates between routes, so the shell never flashes. Show skeletons on first load, not spinners.

## Dev stack

`svx up` starts Mailpit, the API (`cargo run` with `test-hooks`, using `dev/bizkit.yaml`, with `MAILGUN_WEBHOOK_KEY=dev-webhook-key` so you can post signed sample events to the mailing webhook), the demo seed and Vite, on ports of this checkout's own svx slot, so several checkouts can run at once. `svx status` shows the links, `svx logs api` the API log (SQL statements are left out; set `LOG_SQL=1` or pass `--log-sql` to log them), and `svx down` stops it all. Open the app at `http://localhost:<ui port>`, not `127.0.0.1`: the API's base URL is the localhost one, so magic links and the CSRF Origin check use it, while every service still listens on 127.0.0.1. Sign in as `demo@bizkit.test` for the seeded demo workspace (any `@bizkit.test` email works; other emails get no mail, and a second request within a minute sends none) and open the link from the stack's Mailpit.

The `demo-site` service (`bizkit dev demo-site`) serves a small tracked product, "Tallyho", at the Demo site link: landing, pricing, signup and three onboarding steps, with the website project's snippet. Walk it to see live events, people and funnel drop-off in the Website project's analytics. Seeded onboarding events come only with a fresh project file; to refill an old stack, stop the api, delete `.svx/<stack>/data/analytics/<website project id>.db` and run `svx restart api seed`.

`mise run demo:traffic -- --site <demo site url> --api <api url> [--visitors 200]` (`e2e/demo-traffic.ts`) drives headless browsers through the demo site with drop-off at each step, so the funnels fill from real ingest. Pass the `localhost` demo site URL and the api URL from `svx status`. It sends beacons straight to the api with a made-up `X-Forwarded-For` per visitor (through Vite every visitor would be one client) and runs 4 visitors at a time, which stays under the per-site ingest limit. Each run adds events for today, so run it by hand.

## Tests

- Unit tests go in a `#[cfg(test)] mod tests` block in the same file. Integration tests that run the binary go in `tests/`.
- Every new command gets a unit test that captures its output and a CLI test in `crates/bizkit/tests/cli.rs`.
- API behavior tests use `bizkit_testkit::TestApp`: it builds a real app on a temp database with fake clock, mailer, HTTP and blobs, and its clients check every response against the OpenAPI document. The worker doesn't run on its own; call `drain_jobs()`.
- Seed data with `app.seed(Persona::Team, "ns")` instead of building it through the API when the setup isn't what the test is about.
- Use `insta::assert_snapshot!` for multi-line or structured output, and `cargo insta review` to accept intended changes.
