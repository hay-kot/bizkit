// The bizkit analytics snippet. The server fills in the site key, the
// identity mode and the endpoint. Cookieless sites store nothing.
(function (w, d, n) {
  var K = __SITE__, M = __MODE__, E = __ENDPOINT__, S = "bizkit_aid";
  if (w.bizkit && w.bizkit.loaded) return;
  var q = (w.bizkit && w.bizkit.q) || [], ref = d.referrer, last;
  function anon(make) {
    if (M === "cookieless") return null;
    try {
      var s = w.localStorage, a = s.getItem(S);
      if (!a && make) {
        a = w.crypto && crypto.randomUUID ? crypto.randomUUID() : Date.now().toString(36) + Math.random().toString(36).slice(2);
        s.setItem(S, a);
      }
      return a;
    } catch (e) {
      return null;
    }
  }
  function send(name, props, user, traits) {
    if (n.webdriver) return;
    var b = { s: K, n: name, u: location.href, w: w.innerWidth }, a = anon(M === "always" || !!user);
    if (ref) b.r = ref;
    if (props) b.p = props;
    if (a) b.a = a;
    if (user) {
      b.i = String(user);
      if (traits) b.t = traits;
    }
    var j = JSON.stringify(b);
    if (n.sendBeacon && n.sendBeacon(E, j)) return;
    if (w.fetch) fetch(E, { method: "POST", body: j, keepalive: true, credentials: "omit" }).catch(function () {});
  }
  function page() {
    var here = location.pathname + location.search;
    if (here === last) return;
    if (last) ref = "";
    last = here;
    send("pageview");
  }
  var h = w.history, push = h.pushState;
  if (push) {
    h.pushState = function () {
      push.apply(this, arguments);
      page();
    };
    w.addEventListener("popstate", page);
  }
  w.bizkit = {
    loaded: true,
    track: function (name, props) {
      send(String(name), props);
    },
    identify: function (user, traits) {
      if (M !== "cookieless" && user) send("$identify", null, user, traits);
    },
  };
  for (var i = 0; i < q.length; i++) {
    var c = w.bizkit[q[i][0]];
    if (c) c.apply(null, q[i][1] || []);
  }
  page();
})(window, document, navigator);
