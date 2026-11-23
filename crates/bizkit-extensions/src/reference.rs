//! The script API reference that `extensions.hooks.list` returns per point.
//! Keep it in step with `sandbox/prelude.js`. A sandbox test checks that
//! every entry but `ctx.mail.send` exists in a run; the mail integration
//! test covers that one.

use crate::api::{ApiEntry, Capability};

/// One reference row, shown when the point grants `needs`.
struct Row {
    needs: Option<Capability>,
    name: &'static str,
    signature: &'static str,
    summary: &'static str,
    errors: &'static [&'static str],
    example: &'static str,
}

const ROWS: &[Row] = &[
    Row {
        needs: None,
        name: "crypto.digest",
        signature: "(alg: 'sha256' | 'sha1', data: string, opts?: { encoding?: 'hex' | 'base64' }) => string",
        summary: "Hashes the UTF-8 bytes of data, at most 1 MiB. Returns lowercase hex unless opts.encoding is 'base64'.",
        errors: &[
            "alg is not 'sha256' or 'sha1'",
            "opts.encoding is not 'hex' or 'base64'",
            "data is over 1 MiB",
        ],
        example: "const etag = crypto.digest('sha256', JSON.stringify(input));",
    },
    Row {
        needs: None,
        name: "crypto.hmac",
        signature: "(alg: 'sha256' | 'sha1', key: string, data: string, opts?: { keyEncoding?: 'utf8' | 'hex' | 'base64', encoding?: 'hex' | 'base64' }) => string",
        summary: "Signs the UTF-8 bytes of data with key, at most 1 MiB of data. The key is UTF-8 text unless opts.keyEncoding is 'hex' or 'base64'. Returns lowercase hex unless opts.encoding is 'base64'. Use it to sign outbound requests and to check inbound signatures (Stripe, GitHub, Shopify).",
        errors: &[
            "alg is not 'sha256' or 'sha1'",
            "opts.keyEncoding or opts.encoding is not one of the listed values",
            "key is not valid hex or base64 for its keyEncoding",
            "data is over 1 MiB",
        ],
        example: "const sig = crypto.hmac('sha256', ctx.secrets.SIGNING_SECRET, `${timestamp}.${body}`);",
    },
    Row {
        needs: None,
        name: "crypto.timingSafeEqual",
        signature: "(a: string, b: string) => boolean",
        summary: "Compares two strings byte for byte in time that depends only on their lengths. Strings of different lengths are not equal. Use it, not ===, to compare signatures.",
        errors: &[],
        example: "if (!crypto.timingSafeEqual(given, expected)) return { verified: false };",
    },
    Row {
        needs: None,
        name: "crypto.randomUUID",
        signature: "() => string",
        summary: "A random version 4 UUID in lowercase hyphenated form, for idempotency keys and delivery IDs.",
        errors: &[],
        example: "headers['idempotency-key'] = crypto.randomUUID();",
    },
    Row {
        needs: None,
        name: "encoding.base64Encode",
        signature: "(text: string, opts?: { url?: boolean }) => string",
        summary: "Base64 of the UTF-8 bytes of text, with padding. opts.url gives the URL-safe alphabet without padding.",
        errors: &[],
        example: "const auth = 'Basic ' + encoding.base64Encode(`${user}:${ctx.secrets.API_TOKEN}`);",
    },
    Row {
        needs: None,
        name: "encoding.base64Decode",
        signature: "(base64: string, opts?: { url?: boolean }) => string",
        summary: "Decodes base64 (URL-safe with opts.url, padding optional there) and returns the bytes as UTF-8 text.",
        errors: &[
            "the input is not base64",
            "the decoded bytes are not UTF-8 text: scripts have no binary type",
        ],
        example: "const claims = JSON.parse(encoding.base64Decode(token.split('.')[1], { url: true }));",
    },
    Row {
        needs: None,
        name: "encoding.hexEncode",
        signature: "(text: string) => string",
        summary: "Lowercase hex of the UTF-8 bytes of text.",
        errors: &[],
        example: "encoding.hexEncode('hi') // '6869'",
    },
    Row {
        needs: None,
        name: "encoding.hexDecode",
        signature: "(hex: string) => string",
        summary: "Decodes hex, either case, and returns the bytes as UTF-8 text.",
        errors: &[
            "the input is not an even number of hex digits",
            "the decoded bytes are not UTF-8 text",
        ],
        example: "encoding.hexDecode('6869') // 'hi'",
    },
    Row {
        needs: None,
        name: "URL",
        signature: "new (input: string, base?: string) => URL",
        summary: "A WHATWG URL with href, origin, protocol, username, password, host, hostname, port, pathname, search, hash and a live searchParams. href, pathname, search and hash can be set; the other parts are read-only. String(url) and JSON.stringify give href. URL.canParse(input, base?) checks without throwing.",
        errors: &[
            "input is not a valid URL, or a relative one without a valid base",
            "a script assigns a read-only part such as host",
        ],
        example: "const url = new URL('/v1/events', ctx.secrets.API_BASE); url.searchParams.set('source', 'bizkit');",
    },
    Row {
        needs: None,
        name: "URLSearchParams",
        signature: "new (init?: string | Record<string, string> | [string, string][]) => URLSearchParams",
        summary: "Ordered form-encoded pairs: get, getAll, has, append, set, delete, sort, entries, keys, values, forEach, size and toString. A leading ? in a string init is dropped. toString gives application/x-www-form-urlencoded text.",
        errors: &["an array init holds an item that is not a [name, value] pair"],
        example: "const query = new URLSearchParams(input.request.query ?? ''); const page = query.get('page');",
    },
    Row {
        needs: Some(Capability::Log),
        name: "ctx.log",
        signature: "(...values: unknown[]) => void",
        summary: "Adds one line to the run's logs: strings as they are, other values as JSON, joined by spaces. A run keeps at most 50 lines of 500 characters; more are dropped. console.log, info, warn, error and debug do the same.",
        errors: &[],
        example: "ctx.log('posted', res.status);",
    },
    Row {
        needs: Some(Capability::Secrets),
        name: "ctx.secrets",
        signature: "Readonly<Record<string, string>>",
        summary: "The project's secrets by name, frozen. A missing name reads as undefined. Runs redact secret values from logs, errors and output.",
        errors: &[],
        example: "const url = ctx.secrets.SLACK_WEBHOOK_URL; if (!url) throw new Error('set the SLACK_WEBHOOK_URL secret');",
    },
    Row {
        needs: Some(Capability::Fetch),
        name: "ctx.fetch",
        signature: "(url: string, init?: { method?: string, headers?: Record<string, string>, body?: string, json?: unknown, form?: Record<string, string>, timeoutMs?: number }) => Promise<{ status: number, ok: boolean, headers: Record<string, string>, text(): Promise<string>, json(): Promise<unknown> }>",
        summary: "Sends an HTTP request to a public http or https address. method defaults to GET. Give at most one of body (a string, sent as is), json (JSON.stringify, content-type application/json) and form (URL-encoded, content-type application/x-www-form-urlencoded); a content-type in headers wins. Header names are lowercased both ways. ok is true for 2xx; non-2xx statuses resolve, they do not throw. Limits: 5 calls per run, 256 KiB request and response bodies, and each call ends after 5 s, timeoutMs or the run's remaining 10 s budget, whichever is first. Test runs send real requests.",
        errors: &[
            "more than one of body, json and form",
            "body or a header value is not a string",
            "the URL is not absolute http or https, or points at a private, loopback or localhost address",
            "the run already made 5 calls",
            "timeoutMs is 0, the call timed out, or the run has no time left",
            "a body is over 256 KiB",
            "the connection failed (the message names the method and host, never the full URL)",
            "res.json() on a body that is not JSON",
        ],
        example: "const res = await ctx.fetch(ctx.secrets.SLACK_WEBHOOK_URL, { method: 'POST', json: { text: `New submission to ${input.form.name}` } }); if (!res.ok) throw new Error(`Slack answered ${res.status}`);",
    },
    Row {
        needs: Some(Capability::Mail),
        name: "ctx.mail.send",
        signature: "(message: { to: string, subject: string, text: string, html?: string }) => void",
        summary: "Queues one email from the server's sender address; delivery retries on its own. Limits: 3 per run, 50 per script and 200 per workspace in a rolling hour (test runs count), a subject of 1-200 characters on one line, text and html at most 64 KiB each. A failed run retries the whole script, so a retried run sends again; test runs send real mail.",
        errors: &[
            "to is not a bare email address (no display name, angle brackets or control characters)",
            "the subject is empty, over 200 characters, or holds line breaks or control characters",
            "text or html is over 64 KiB",
            "the run already sent 3, the script 50 or the workspace 200 in the last hour",
            "to, subject or text is missing or not a string",
        ],
        example: "ctx.mail.send({ to: 'owner@example.com', subject: `New lead: ${input.submission.data.email}`, text: JSON.stringify(input.submission.data, null, 2) });",
    },
];

/// The globals, then the `ctx` members `capabilities` grant.
pub(crate) fn entries(capabilities: &[Capability]) -> Vec<ApiEntry> {
    ROWS.iter()
        .filter(|row| row.needs.is_none_or(|needs| capabilities.contains(&needs)))
        .map(|row| ApiEntry {
            name: row.name.to_owned(),
            signature: row.signature.to_owned(),
            summary: row.summary.to_owned(),
            errors: row.errors.iter().map(|&error| error.to_owned()).collect(),
            example: row.example.to_owned(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_points_get_globals_and_log_only() {
        let names: Vec<String> =
            entries(&[Capability::Log]).into_iter().map(|entry| entry.name).collect();
        assert!(names.contains(&"crypto.hmac".to_owned()), "{names:?}");
        assert!(names.contains(&"ctx.log".to_owned()), "{names:?}");
        assert!(!names.iter().any(|name| name == "ctx.fetch" || name == "ctx.mail.send"));
    }
}
