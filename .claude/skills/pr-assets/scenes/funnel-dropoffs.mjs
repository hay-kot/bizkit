// pr-record .claude/skills/pr-assets/scenes/funnel-dropoffs.mjs --url http://localhost:5203
//
// Signs in as the demo user through the dev stack's Mailpit, then opens the
// Website project's Onboarding funnel and the people who dropped off at step 3.
// Set MAILPIT_URL when the stack's Mailpit is not on port 8055.
export const start = "/sign-in";
export const posterAt = 21;

const EMAIL = "demo@bizkit.test";
const MAILPIT = process.env.MAILPIT_URL ?? "http://localhost:8055";
const LINK = /https?:\/\/\S+\/sign-in\/verify\?token=[\w-]+(?:&next=[^\s"<]+)?/;

// The stack sends one sign-in mail per address a minute, so only a mail sent
// after this request counts; an older link may be spent.
async function signInLink(since) {
  const query = encodeURIComponent(`to:"${EMAIL}"`);
  for (let i = 0; i < 30; i++) {
    const res = await fetch(`${MAILPIT}/api/v1/search?query=${query}&limit=5`);
    const { messages = [] } = await res.json();
    for (const m of messages) {
      if (new Date(m.Created).getTime() < since) continue;
      const body = await (await fetch(`${MAILPIT}/api/v1/message/${m.ID}`)).json();
      const match = LINK.exec(`${body.Text}\n${body.HTML ?? ""}`);
      if (match) return new URL(match[0]);
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error(
    `no new sign-in mail for ${EMAIL} in ${MAILPIT}; the stack sends one a minute, so wait and rerun`,
  );
}

export default async function ({ page, d, baseUrl }) {
  await d.sleep(500);
  await d.click(page.getByLabel("Email"));
  await d.type(EMAIL);
  const since = Date.now() - 1000;
  await d.click(page.getByRole("button", { name: "Email me a link" }));
  await page.getByTestId("link-sent").waitFor();
  await d.sleep(1200);

  const link = await signInLink(since);
  await page.goto(new URL(link.pathname + link.search, baseUrl).href);
  await page.getByTestId("signed-in-as").waitFor();
  await d.settled();
  await d.sleep(800);

  await d.click(page.getByRole("link", { name: /Demo/ }).first());
  await page.getByTestId("project-name").waitFor();
  await d.settled();
  if ((await page.getByTestId("project-name").textContent())?.trim() !== "Website") {
    await d.click(page.getByRole("button", { name: "Switch project" }));
    await d.click(page.getByRole("menuitem", { name: "Website" }));
    await d.settled();
  }

  await d.click(page.getByRole("navigation", { name: "Project" }).getByRole("link", { name: "Analytics" }));
  await d.settled();
  await d.sleep(1200);
  await d.click(page.getByRole("tab", { name: "Funnels" }).or(page.getByRole("link", { name: "Funnels" })).first());
  await d.settled();
  await d.sleep(800);
  await d.click(page.getByRole("link", { name: "Onboarding" }).first());
  await d.settled();
  await d.sleep(1500);

  const steps = page.getByRole("list", { name: "Funnel steps" });
  await d.click(steps.getByRole("link", { name: /dropped off/ }).nth(1));
  await page.getByText(/Dropped before step 3/).waitFor();
  await d.settled();
  await d.sleep(2500);
  await d.scrollTo(420);
  await d.sleep(1500);
}
