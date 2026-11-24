# AGENTS.md

Guidance for coding agents working in this repository.

## Before you finish

Run `mise run check`. It is what CI runs, and it must pass: formatting, clippy with warnings as errors (with and without the `test-hooks` feature), tests, docs, cargo-deny, cargo-shear, typos, the sqlx query data (`db:check`), the generated API client (`gen:check`) and the web checks (`web:check`). CI also runs `mise run test:e2e`.

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
- Module tables carry `workspace_id`, and every query filters on it.
- Module config lives under `modules.<name>` in the config file, parsed into the module's own struct with `deny_unknown_fields`. Analytics reads `geoip_path` (a MaxMind-format country file; without it countries are unknown) `raw_retention_days` (default 90, at least 7) and `flush_ms` (the write window, default 100, 10 to 250).
- Public routes that need the visitor's address take the `ClientIp` extractor. It reads the peer address, or, when the top-level `trusted_proxies` is n, the nth `X-Forwarded-For` entry from the right. Set it to the number of proxies in front of bizkit that append the header (the dev stack has one, Vite); a higher number lets clients claim any address.
- Analytics counts by UTC day. Ingest (the snippet, the beacon and server events) goes through `Files::submit`, the one write path: it queues events for the project's buffered writer, which commits them every `flush_ms` or at 1,000 queued. A crash loses at most one window; a clean shutdown loses none. Reports read `analytics_daily` up to `rolled_through` and raw events after it, so a report gives the same numbers before and after the rollup job.
- Analytics keeps events, rollups, people, aliases and conversions in one SQLite file per project (`data_dir/analytics/<project_id>.db`, migrations in `migrations-project/`). Sites, goals, funnels and salts stay in the app database. A query must not join app tables with file tables: read the app rows first, then the file. Deleting a project publishes `ProjectDeleted`, and analytics removes the file.
- A module with buffered work registers a `Lifecycle` (`cx.lifecycle`): `flush` runs on `Platform::flush_modules` and before jobs drain, `stop` on shutdown.
- Analytics tests build with `Analytics::manual_flush()` (no timer), call `app.flush()` after ingest and before every read, and open a project's file with `bizkit_analytics::testing::project_pool(app.data_dir(), project_id)`.
- Migrations live in the module crate's `migrations/` and return from `Module::migrations` as `sqlx::migrate!()`. Versions must be unique across all crates; a test checks it.

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

## Database and generated code

- Queries use the `sqlx::query!` macros. Builds read the committed `.sqlx/` (`SQLX_OFFLINE=true` in `mise.toml`). After changing a query or a migration, run `mise run db:prepare` and commit `.sqlx/`.
- After changing an operation or a schema type, run `mise run gen`. It rewrites `openapi.json` and `web/src/api/gen`; commit both.

## Frontend

- Vue 3, Tailwind v4 and DaisyUI 5. Use Tailwind and DaisyUI classes only: ESLint rejects unknown classes, arbitrary values (`w-[13px]`), raw palette colors (`bg-blue-500`) and inline styles. Use DaisyUI semantic colors (`primary`, `base-200`, `error`).
- Colors are defined only in `web/src/styles/theme.css`; stylelint rejects hex, named and functional colors elsewhere.
- Only `web/src/queries/**` and `web/src/api/client.ts` import the generated client in `web/src/api/gen`. Pages use the composables in `src/queries`.
- Disabling an ESLint rule needs a reason: `// eslint-disable-next-line rule -- why`. The one inline style today is the floating list in `Select.vue`, which needs coordinates.
- Layout: `/` is the user home (workspaces, invitations, create). Inside a workspace, `layouts/WorkspaceLayout.vue` gives the sidebar: workspace switcher on top, then the project switcher with the selected project's pages, then the workspace meta pages, then the user menu. `/w/:workspaceId` lands on the current project's first page (the last project used, else the first), or on Projects when there are none.
- A feature's UI is a web module: `web/src/modules/<name>.ts` calls `defineWebModule({ name, projectRoutes, workspaceRoutes, nav })` and is listed in `modules/index.ts`. The router and the sidebar read the list, so a module never edits the shell. Module data belongs to a project, so feature pages go in `projectRoutes` (mounted at `/w/:workspaceId/p/:projectId/`, params arrive as props) with nav items in `section: 'project'`; they show under the sidebar's project switcher. `section: 'workspace'` is for workspace meta (Team, API keys, Settings) pinned to the bottom. Sidebar `order` 0-99 is core's; modules start at 100; Settings stays last. A unit test checks that nav items point at registered routes of their section and nothing collides.
- **Command palette.** A module adds rows with `palette: (cx) => rows` on `defineWebModule`. `PaletteRows` in the workspace layout calls it in a component setup (so query composables work) and calls it again when the workspace, project or admin flag changes. Nav items become rows on their own. Rows name the object or action (`Contact`, `New form`), never the path: `group` is the container (`Forms`), `kind` a muted type word (`form`), and verbs go in `keywords`. Objects use `scope: 'goto'` and ids like `forms.form:<id>`; actions keep the default scope. Return `[]` when the rows need a project or admin rights the user lacks. A page can add page-local rows with `usePaletteRows(rows)`; they go away with the page. A "New ..." action links to its list page with `?new=1`, and the page calls `useCreateIntent(openPanel)`.
- Pages follow the archetypes in `web/DESIGN.md` (list, detail, settings, report): read it before you add or restyle a page. Detail and edit pages are one column.
- Building blocks, and what ESLint enforces outside `src/components/ui`:

  | Need | Use | Enforced by |
  |---|---|---|
  | A page | `Page` (sets the width) > `PageHeader` > `PageSection`s and `Card`s | `grid-cols-*`, `col-span-*`, `columns-*`, `grid-flow-col` and `max-w-*` are errors in `src/pages` |
  | A detail page's header actions | `DetailActions`: page buttons, Save (its label is the save state) and a `#menu` of `MenuItem`s (`danger` for delete, last) | Pages can't import `ActionsMenu` |
  | A list of rows | `DataTable` with `#cell-<key>` slots; it handles loading, error and empty | `<table>` and DaisyUI `table*` classes are errors |
  | A choice | `Select` | `<select>`, `input[type=radio]` and DaisyUI `select*` classes are errors |
  | A tick box | `Checkbox` | `input[type=checkbox]` and DaisyUI `checkbox`/`toggle` classes are errors |
  | A create or edit form | `FormPanel` (side panel, or `variant="dialog"`), `Field`, `SubmitButton` | Pages can't import `Overlay`; DaisyUI `modal` classes are errors |
  | A menu | `Menu` and `MenuItem` | DaisyUI `dropdown` classes are errors |
  | Waiting | `SubmitButton` `pending`, `SkeletonRows`, `Spinner` | DaisyUI `loading` classes are errors |
  | Confirm, run, report | `runAction({ confirm, run, success })` | Pages can't import `lib/confirm`; `alert`/`confirm`/`prompt` are errors |
  | A date | `formatDate`, `formatShortDate`, `isPast` from `lib/format.ts` | `toLocale*String` and `new Intl.*` are errors elsewhere |
  | A number | `formatNumber`, `formatCompactNumber`, `formatPercent` from `lib/format.ts` | `toLocale*String` and `new Intl.*` are errors elsewhere |
| A UTC day (analytics buckets) | `formatUtcDay`, `formatShortUtcDay` from `lib/format.ts`; the local-time formatters shift it a day west of UTC | |
  | Tabs between a feature's pages | `Tabs` (route links) | |
  | A report's date range | `DateRangePicker`, with `presetRange` and `previousPeriod` from `lib/range.ts`; `useReportRange` keeps the range and compare flag in the URL | |
  | KPIs with a change | `StatTiles` | |
  | Numbers over time | `TimeSeriesChart` (SVG, no chart library) | Inline styles and arbitrary values are errors |
  | A ranked list with share bars | `BarList` (bars are `progress` elements) | Inline styles and arbitrary values are errors |
  | Report panels side by side | `ReportGrid` | `grid-cols-*` is an error in `src/pages` |
  | API data | the composables in `src/queries` | Pages and layouts can't import `@pinia/colada` or `src/api/gen` |
  | A name with a slug | `useSlugFromName()` | |
  | A page or action reachable by keyboard | a module `palette` source, or `usePaletteRows` for page-local rows | |

- When a page needs something these don't cover, add a primitive to `src/components/ui` (or a feature component that wraps one) instead of reaching for the raw element.
- Pages load eagerly and only the content area animates between routes, so the shell never flashes. Show skeletons on first load, not spinners.

## Dev stack

`svx up` starts Mailpit, the API (`cargo run` with `test-hooks`, using `dev/bizkit.yaml`), the demo seed and Vite, on ports of this checkout's own svx slot, so several checkouts can run at once. `svx status` shows the links, `svx logs api` the API log, and `svx down` stops it all. Open the app at `http://localhost:<ui port>`, not `127.0.0.1`: the API's base URL is the localhost one, so magic links and the CSRF Origin check use it, while every service still listens on 127.0.0.1. Sign in as `demo@bizkit.test` for the seeded demo workspace (any `@bizkit.test` email works; other emails get no mail, and a second request within a minute sends none) and open the link from the stack's Mailpit.

The `demo-site` service (`bizkit dev demo-site`) serves a small tracked product, "Tallyho", at the Demo site link: landing, pricing, signup and three onboarding steps, with the website project's snippet. Walk it to see live events, people and funnel drop-off in the Website project's analytics. Seeded onboarding events come only with a fresh project file; to refill an old stack, stop the api, delete `.svx/<stack>/data/analytics/<website project id>.db` and run `svx restart api seed`.

## Tests

- Unit tests go in a `#[cfg(test)] mod tests` block in the same file. Integration tests that run the binary go in `tests/`.
- Every new command gets a unit test that captures its output and a CLI test in `crates/bizkit/tests/cli.rs`.
- API behavior tests use `bizkit_testkit::TestApp`: it builds a real app on a temp database with fake clock, mailer, HTTP and blobs, and its clients check every response against the OpenAPI document. The worker doesn't run on its own; call `drain_jobs()`.
- Seed data with `app.seed(Persona::Team, "ns")` instead of building it through the API when the setup isn't what the test is about.
- Use `insta::assert_snapshot!` for multi-line or structured output, and `cargo insta review` to accept intended changes.
