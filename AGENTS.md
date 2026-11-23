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
- Migrations live in the module crate's `migrations/` and return from `Module::migrations` as `sqlx::migrate!()`. Versions must be unique across all crates; a test checks it.

## Hooks

Modules let workspace scripts run at **hook points** they declare. Core owns the contract (`bizkit_core::hooks`); the `extensions` module runs the scripts in QuickJS sandboxes. A module never depends on extensions, and without it every point is inert.

- Declare points in `register`. IDs start with the module name (`forms.pre_validate`). Keep the `HookPoint` consts in the module's `hooks.rs` with a `sample` input.
- **Target kinds.** When scripts should pick which of the module's entities they run for (form A and C, not B), declare a `TargetKind` (`forms.form`, owned like a point ID) with `cx.hook_target_kind(&KIND, resolver)` before the points, and set `target: Some(&KIND)` on them. The `TargetResolver` finds entities by ID (returning canonical IDs) and lists a project's entities for the picker. Every hook call then passes the entity in `HookTarget.entity`.
- A script's target set is either empty (every entity of the kind) or a list of entities. Async points run every matching script. Sync points run one script per entity: one that names the entity wins over the one for all, and the API refuses (409) a second script naming the same entity on a sync point, or a second one for all.
- **Sync** (`cx.sync_hook`): runs inside the request through `platform.call_sync_hook(&POINT, target, &input).await`, with a budget of at most 100 ms and an `output` check on the returned value. Sync hooks fail open: on any failure the call gives `HookResult::Continue`, so a broken script never blocks the request. Scripts get `ctx.log` only. One script per entity and point (see target kinds above).
- **Async** (`cx.async_hook::<Event, _>(POINT, source)`): runs as a retried job for each event (3 attempts; a failed attempt reruns the whole script, so scripts must tolerate repeats). The `HookSource` gives the target project and rebuilds the input when the run starts, so job payloads hold IDs only and a deleted row skips the run. Scripts also get `ctx.secrets` and `ctx.fetch` (public addresses only).
- Inputs are plain JSON with no secrets or internal keys. Leave control fields out. Cap any text you take back from a script.
- Tests use `FakeHooks` with `TestAppBuilder::hook_runner` to script outcomes per point. Test that every `RunStatus` failure leaves the request working.
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
- Pages follow the archetypes in `web/DESIGN.md` (list, detail, settings): read it before you add or restyle a page. Detail and edit pages are one column.
- Building blocks, and what ESLint enforces outside `src/components/ui`:

  | Need | Use | Enforced by |
  |---|---|---|
  | A page | `Page` (sets the width) > `PageHeader` > `PageSection`s and `Card`s | `grid-cols-*`, `col-span-*` and `max-w-*` are errors in `src/pages` |
  | Secondary or destructive header actions | `ActionsMenu` with `MenuItem` (`danger` for delete, last) | |
  | Save state of an inline editor | `SaveStatus` next to the header Save | |
  | A list of rows | `DataTable` with `#cell-<key>` slots; it handles loading, error and empty | `<table>` and DaisyUI `table*` classes are errors |
  | A choice | `Select` | `<select>`, `input[type=radio]` and DaisyUI `select*` classes are errors |
  | A tick box | `Checkbox` | `input[type=checkbox]` and DaisyUI `checkbox`/`toggle` classes are errors |
  | A create or edit form | `FormPanel` (side panel, or `variant="dialog"`), `Field`, `SubmitButton` | Pages can't import `Overlay`; DaisyUI `modal` classes are errors |
  | A menu | `Menu` and `MenuItem` | DaisyUI `dropdown` classes are errors |
  | Waiting | `SubmitButton` `pending`, `SkeletonRows`, `Spinner` | DaisyUI `loading` classes are errors |
  | Confirm, run, report | `runAction({ confirm, run, success })` | Pages can't import `lib/confirm`; `alert`/`confirm`/`prompt` are errors |
  | A date | `formatDate`, `formatShortDate`, `isPast` from `lib/format.ts` | `toLocale*String` and `new Intl.*` are errors elsewhere |
  | API data | the composables in `src/queries` | Pages and layouts can't import `@pinia/colada` or `src/api/gen` |
  | A name with a slug | `useSlugFromName()` | |

- When a page needs something these don't cover, add a primitive to `src/components/ui` (or a feature component that wraps one) instead of reaching for the raw element.
- Pages load eagerly and only the content area animates between routes, so the shell never flashes. Show skeletons on first load, not spinners.

## Dev stack

`svx up` starts Mailpit, the API (`cargo run` with `test-hooks`, using `dev/bizkit.yaml`), the demo seed and Vite, on ports of this checkout's own svx slot, so several checkouts can run at once. `svx status` shows the links, `svx logs api` the API log, and `svx down` stops it all. Sign in as `demo@bizkit.test` for the seeded demo workspace (any `@bizkit.test` email works) and open the link from the stack's Mailpit.

## Tests

- Unit tests go in a `#[cfg(test)] mod tests` block in the same file. Integration tests that run the binary go in `tests/`.
- Every new command gets a unit test that captures its output and a CLI test in `crates/bizkit/tests/cli.rs`.
- API behavior tests use `bizkit_testkit::TestApp`: it builds a real app on a temp database with fake clock, mailer, HTTP and blobs, and its clients check every response against the OpenAPI document. The worker doesn't run on its own; call `drain_jobs()`.
- Seed data with `app.seed(Persona::Team, "ns")` instead of building it through the API when the setup isn't what the test is about.
- Use `insta::assert_snapshot!` for multi-line or structured output, and `cargo insta review` to accept intended changes.
