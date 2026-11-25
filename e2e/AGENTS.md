# e2e

Playwright tests against a real `bizkit serve` and Mailpit. Run them with `mise run test:e2e` from the repo root; pass Playwright flags after `--`, for example `mise run test:e2e -- --project smoke`.

- `scripts/test-e2e.sh` owns the server: it builds the web app and the binary (with `test-hooks` and, until the signing sender UI lands, `signing`), starts Mailpit, writes a config and exports `BASE_URL`, `MAILPIT_URL` and `E2E_SEED_CMD`. The Playwright config has no `webServer`.
- Seed data per test with `test.use({ persona: 'team' })`. The fixture seeds a fresh namespace for each test and retry, and signs the page in as `admin` (change it with `test.use({ as: 'member' })`). `world` holds the manifest: IDs, emails, session tokens, API keys.
- Emails you make up go through the `namespace` fixture, so parallel tests never collide. `*@signup.e2e.test` may sign up; any other email needs an invitation.
- Read mail with `fixtures/mailpit.ts`. Open links by path (`page.goto(path)`): the server's base URL is the one in the mail.
- Smoke tests call `expectCleanPage`: no horizontal overflow at 1440 and 390 px, no uncaught errors, no axe WCAG A/AA violations.
- Projects: `smoke` (every page renders clean), `workflows` (multi-step journeys across accounts), `features` (one feature at a time), and `isolated`, which runs after the others finish, one test at a time, for tests that change shared state: Mailpit chaos (`PUT /api/v1/chaos` makes SMTP reject mail; reset it in a `finally`) or big server clock moves.
- The seed command keeps real time, so tests that move the server clock (`POST /__test/clock`) must stay under the 30-day session TTL. Test expiry edge cases in Rust with `FakeClock` instead.
