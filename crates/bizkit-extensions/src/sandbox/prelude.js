// Runs before each script. It removes the eval and Function globals,
// installs console and the string-only globals (crypto, encoding, URL,
// URLSearchParams), and returns the frozen input and ctx for the script's
// default export. `(() => {}).constructor` still compiles code; that is
// fine, because it runs in the same runtime under the same limits.
(function (host, inputJson, secretsJson) {
  'use strict';
  delete globalThis.eval;
  delete globalThis.Function;

  const deepFreeze = (value) => {
    if (value !== null && typeof value === 'object' && !Object.isFrozen(value)) {
      Object.freeze(value);
      for (const key of Object.keys(value)) deepFreeze(value[key]);
    }
    return value;
  };

  const text = (value) => {
    if (typeof value === 'string') return value;
    try {
      const json = JSON.stringify(value);
      return json === undefined ? String(value) : json;
    } catch {
      return String(value);
    }
  };

  const define = (name, value) => {
    Object.defineProperty(globalThis, name, { value, writable: false, configurable: false });
  };

  const log = (...args) => {
    host.log(args.map(text).join(' '));
  };
  define('console', Object.freeze({ log, info: log, warn: log, error: log, debug: log }));

  define(
    'crypto',
    Object.freeze({
      randomUUID: () => host.uuid(),
      digest: (alg, data, opts) =>
        host.digest(String(alg), String(data), String(opts?.encoding ?? 'hex')),
      hmac: (alg, key, data, opts) =>
        host.hmac(
          String(alg),
          String(key),
          String(data),
          String(opts?.keyEncoding ?? 'utf8'),
          String(opts?.encoding ?? 'hex'),
        ),
      timingSafeEqual: (a, b) => host.timingSafeEqual(String(a), String(b)),
    }),
  );

  define(
    'encoding',
    Object.freeze({
      base64Encode: (value, opts) => host.b64Encode(String(value), Boolean(opts?.url)),
      base64Decode: (value, opts) => host.b64Decode(String(value), Boolean(opts?.url)),
      hexEncode: (value) => host.hexEncode(String(value)),
      hexDecode: (value) => host.hexDecode(String(value)),
    }),
  );

  // A URL's searchParams write back to it through this map, and the URL
  // reloads them through `reload` when its query changes, so a script
  // keeps one live object, as in WHATWG.
  const owners = new WeakMap();
  let reload;

  class URLSearchParams {
    #pairs;
    static {
      reload = (params, query) => {
        params.#pairs = JSON.parse(host.formDecode(query));
      };
    }
    constructor(init) {
      if (init === undefined || init === null) {
        this.#pairs = [];
      } else if (typeof init === 'string') {
        this.#pairs = JSON.parse(host.formDecode(init));
      } else if (init instanceof URLSearchParams) {
        this.#pairs = [...init];
      } else if (Symbol.iterator in Object(init)) {
        this.#pairs = Array.from(init, (pair) => {
          const items = Array.from(pair);
          if (items.length !== 2) throw new TypeError('each pair needs a name and a value');
          return [String(items[0]), String(items[1])];
        });
      } else {
        this.#pairs = Object.entries(init).map(([name, value]) => [name, String(value)]);
      }
    }
    #changed() {
      owners.get(this)?.(this.toString());
    }
    get size() {
      return this.#pairs.length;
    }
    append(name, value) {
      this.#pairs.push([String(name), String(value)]);
      this.#changed();
    }
    delete(name, value) {
      const n = String(name);
      this.#pairs = this.#pairs.filter(
        ([k, v]) => k !== n || (value !== undefined && v !== String(value)),
      );
      this.#changed();
    }
    get(name) {
      const found = this.#pairs.find(([k]) => k === String(name));
      return found === undefined ? null : found[1];
    }
    getAll(name) {
      return this.#pairs.filter(([k]) => k === String(name)).map(([, v]) => v);
    }
    has(name, value) {
      return this.#pairs.some(([k, v]) => k === String(name) && (value === undefined || v === String(value)));
    }
    set(name, value) {
      const n = String(name);
      const at = this.#pairs.findIndex(([k]) => k === n);
      if (at === -1) {
        this.#pairs.push([n, String(value)]);
      } else {
        this.#pairs[at] = [n, String(value)];
        this.#pairs = this.#pairs.filter(([k], i) => k !== n || i <= at);
      }
      this.#changed();
    }
    sort() {
      this.#pairs.sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
      this.#changed();
    }
    forEach(callback, thisArg) {
      for (const [k, v] of this.#pairs) callback.call(thisArg, v, k, this);
    }
    *entries() {
      for (const [k, v] of this.#pairs) yield [k, v];
    }
    *keys() {
      for (const [k] of this.#pairs) yield k;
    }
    *values() {
      for (const [, v] of this.#pairs) yield v;
    }
    [Symbol.iterator]() {
      return this.entries();
    }
    toString() {
      return host.formEncode(JSON.stringify(this.#pairs));
    }
  }

  class URL {
    #parts;
    #params;
    constructor(input, base) {
      this.#parts = JSON.parse(host.parseUrl(String(input), base === undefined ? null : String(base)));
    }
    static canParse(input, base) {
      try {
        new URL(input, base);
        return true;
      } catch {
        return false;
      }
    }
    #set(part, value) {
      this.#parts = JSON.parse(host.setUrl(this.#parts.href, part, String(value)));
    }
    get href() {
      return this.#parts.href;
    }
    set href(value) {
      this.#parts = JSON.parse(host.parseUrl(String(value), null));
      if (this.#params !== undefined) reload(this.#params, this.#parts.search);
    }
    get origin() {
      return this.#parts.origin;
    }
    get protocol() {
      return this.#parts.protocol;
    }
    get username() {
      return this.#parts.username;
    }
    get password() {
      return this.#parts.password;
    }
    get host() {
      return this.#parts.host;
    }
    get hostname() {
      return this.#parts.hostname;
    }
    get port() {
      return this.#parts.port;
    }
    get pathname() {
      return this.#parts.pathname;
    }
    set pathname(value) {
      this.#set('pathname', value);
    }
    get search() {
      return this.#parts.search;
    }
    set search(value) {
      this.#set('search', value);
      if (this.#params !== undefined) reload(this.#params, this.#parts.search);
    }
    get hash() {
      return this.#parts.hash;
    }
    set hash(value) {
      this.#set('hash', value);
    }
    get searchParams() {
      if (this.#params === undefined) {
        this.#params = new URLSearchParams(this.#parts.search);
        owners.set(this.#params, (query) => this.#set('search', query));
      }
      return this.#params;
    }
    toString() {
      return this.href;
    }
    toJSON() {
      return this.href;
    }
  }

  Object.freeze(URLSearchParams.prototype);
  Object.freeze(URL.prototype);
  define('URLSearchParams', Object.freeze(URLSearchParams));
  define('URL', Object.freeze(URL));

  const ctx = { log };
  if (host.fetch !== undefined) {
    // Turns the json and form options into a body and a default
    // content-type, so the host only ever sees a string body.
    const request = (init) => {
      const { json, form, ...rest } = init ?? {};
      const given = [rest.body, json, form].filter((value) => value !== undefined).length;
      if (given > 1) throw new TypeError('fetch takes one of body, json and form');
      const headers = Object.fromEntries(
        Object.entries(rest.headers ?? {}).map(([name, value]) => {
          if (typeof value !== 'string') throw new TypeError(`fetch header ${name} must be a string`);
          return [name.toLowerCase(), value];
        }),
      );
      if (rest.body !== undefined && typeof rest.body !== 'string') {
        throw new TypeError('fetch body must be a string; use json for objects');
      }
      let body = rest.body;
      let type;
      if (json !== undefined) {
        body = JSON.stringify(json);
        type = 'application/json';
      } else if (form !== undefined) {
        body = new URLSearchParams(form).toString();
        type = 'application/x-www-form-urlencoded';
      }
      if (type !== undefined && headers['content-type'] === undefined) headers['content-type'] = type;
      return { method: rest.method, headers, body, timeoutMs: rest.timeoutMs };
    };
    ctx.fetch = async (url, init) => {
      const reply = JSON.parse(host.fetch(String(url), JSON.stringify(request(init))));
      if (reply.error !== undefined) throw new Error(reply.error);
      const body = reply.body;
      return Object.freeze({
        status: reply.status,
        ok: reply.status >= 200 && reply.status < 300,
        headers: Object.freeze(reply.headers),
        text: async () => body,
        json: async () => JSON.parse(body),
      });
    };
  }
  if (host.mail !== undefined) {
    ctx.mail = Object.freeze({
      send: (message) => {
        const { to, subject, text, html } = message ?? {};
        const error = host.mail(JSON.stringify({ to, subject, text, html }));
        if (error !== undefined && error !== null) throw new Error(error);
      },
    });
  }
  if (secretsJson !== undefined) {
    // A null value is a secret that doesn't open with the instance key: it
    // throws when read, so scripts that never touch it keep running.
    const secrets = {};
    for (const [name, value] of Object.entries(JSON.parse(secretsJson))) {
      if (value === null) {
        Object.defineProperty(secrets, name, {
          enumerable: true,
          get() {
            throw new Error(
              `the secret ${name} doesn't open with this instance's secrets key; set it again`,
            );
          },
        });
      } else {
        secrets[name] = value;
      }
    }
    ctx.secrets = Object.freeze(secrets);
  }
  return [deepFreeze(JSON.parse(inputJson)), Object.freeze(ctx)];
})
