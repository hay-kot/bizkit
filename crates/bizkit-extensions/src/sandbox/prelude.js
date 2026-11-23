// Runs before each script. It removes code generation from the globals,
// installs console, and returns the frozen input and ctx for the script's
// default export.
(function (hostLog, hostFetch, inputJson, secretsJson) {
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

  const log = (...args) => {
    hostLog(args.map(text).join(' '));
  };
  globalThis.console = Object.freeze({ log, info: log, warn: log, error: log, debug: log });

  const ctx = { log };
  if (hostFetch !== undefined) {
    ctx.fetch = async (url, init) => {
      const reply = JSON.parse(hostFetch(String(url), JSON.stringify(init ?? {})));
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
  if (secretsJson !== undefined) ctx.secrets = deepFreeze(JSON.parse(secretsJson));
  return [deepFreeze(JSON.parse(inputJson)), Object.freeze(ctx)];
})
