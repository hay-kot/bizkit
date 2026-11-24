---
name: pr-assets
description: Take screenshots and screen recordings of bizkit, upload them with pra, and embed them in a pull request body. Use when a PR changes something visible (a page, layout, a multi-step flow, CLI output) or when the user asks for screenshots, a recording, a demo video, or images in a PR.
---

# Screenshots and recordings for pull requests

`pra` uploads files to a public bucket and prints stable URLs. Its config lives in `~/.config/prassets/config.yaml` and belongs to the user: never edit it, and never write a credential anywhere. Only repos listed in that config can upload, and bizkit is listed.

## Before you start

Run `pra config check` in the repo. If a step fails, tell the user what failed and stop. Do not work around it with another bucket or a hand-written config.

Start the dev stack with `svx up` (or `svx restart api ui` after a change) and read the links from `svx status`. This checkout's slot serves the app at `http://localhost:5203` and Mailpit at `http://localhost:8055`; another checkout has other ports, so take them from `svx status`.

## Rules

- The bucket is public. Show only the seeded demo data (`bizkit dev seed-demo`, the Demo Co workspace) or data you made up. No real customer data, tokens, API keys, secrets, internal URLs, or anything from a production database. Hide or avoid pages that show an API key or a project secret.
- Upload only the few images and videos that show the change. One before and after pair usually beats ten screenshots.
- `pra put` never overwrites. To replace a file, upload it under a new name. Do not pass `--force` unless the user asks.
- Use lower case file names with `-`, such as `funnel-dropoffs-after.png`. Save them in your scratchpad, not the repo.
- Assets expire after 30 days, so images in older PRs break. That is expected.

## Signing in

bizkit signs in by emailed link. Open the app at `http://localhost:<ui port>`, never `127.0.0.1`: the API's base URL is the localhost one, so magic links and the CSRF Origin check use it.

1. Go to `/sign-in`, fill Email with `demo@bizkit.test` and press "Email me a link".
2. Read the newest mail to that address from Mailpit's API (`GET /api/v1/search?query=to:"demo@bizkit.test"`, then `GET /api/v1/message/<ID>`) and take the `/sign-in/verify?token=...` link from its text.
3. Open that link's path and query on the app's origin.

The stack sends one sign-in mail per address a minute. A second sign-in within a minute gets no mail, and an older link may be spent, so only use a mail that arrived after your request. `scenes/funnel-dropoffs.mjs` does all of this. For several screenshot runs, sign in once, save `context.storageState({ path })` to your scratchpad, and load it in later runs instead of asking for another mail.

The demo user's last-used project decides where `/w/<id>` lands, so switch to the project you need (the "Switch project" button) rather than assume it.

## Screenshots

1. Take the screenshot with the Playwright MCP, the browser tool, or a short Playwright script. Use the same data and state for before and after. When the layout changed, take a phone width (375px) and a desktop width (1280px).
2. Look at the image yourself before you upload it.
3. Upload: `pra put funnel-dropoffs.png` prints the URL, and `pra --format md put a.png b.png` prints Markdown.

The `pr` type keys uploads by branch, so you can upload before `gh pr create`.

## Recordings

Record a multi-step flow (more than one click or a state change over time) with `pr-record`. It drives the app with Playwright, draws a cursor that glides to each click (headless Chromium has none), and writes an H.264 MP4 and a JPEG poster.

1. Write a scene: a small ES module that drives the page with Playwright locators. `pr-record --help` shows the format. `scenes/funnel-dropoffs.mjs` next to this file is a full bizkit one: it signs in through Mailpit, opens the Website project's Onboarding funnel and the people who dropped off before step 3. Copy it to your scratchpad and change the steps after sign-in. Put setup you don't want in the video in `prepare`, which runs before the recording starts.
2. Run it from the repo root, so `pra` sees the repo and branch:

   ```sh
   pr-record .claude/skills/pr-assets/scenes/funnel-dropoffs.mjs --url http://localhost:5203 --upload
   ```

   Set `MAILPIT_URL` when Mailpit is not on port 8055. `--phone` records at 375px, `--dark` uses the dark color scheme, `--hide <css>` hides an element, and `--help` lists the rest.
3. Pull a few frames with `ffmpeg -ss <seconds> -i <file>.mp4 -frames:v 1 frame.jpg` and look at them, and at the poster, before you upload.
4. `--upload` puts both files under this branch and prints a poster image that links to the video. Paste that line into the PR body.

Keep a recording under about 30 seconds.

GitHub plays inline only the videos it hosts. A `<video>` tag that points at another host is blocked, and a bare `.mp4` URL renders as a link, so reviewers see the poster and a click opens the video.

For CLI output, `vhs` (where installed) renders a tape to a GIF or MP4, then `pra put` it.

## The PR body

`pra md` prints Markdown for every asset of this branch. Paste the lines you need under a Screenshots heading after the summary, with a short caption that says what each image shows.
