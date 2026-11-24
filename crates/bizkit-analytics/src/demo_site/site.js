// The demo product's tracking. It queues calls until the deferred bizkit
// snippet loads, then moves to the next page with location.assign so each
// step is a real pageview.
window.bizkit = window.bizkit || {
  q: [],
  track: function () {
    this.q.push(["track", arguments]);
  },
  identify: function () {
    this.q.push(["identify", arguments]);
  },
};

// A walk from the landing page is a new visitor, not the last signup. This
// runs before the deferred snippet sends the landing pageview.
if (location.pathname === "/") {
  try {
    localStorage.removeItem("bizkit_aid");
  } catch (e) {}
}

(function () {
  function form(id, next, run) {
    var el = document.getElementById(id);
    if (!el) return;
    el.addEventListener("submit", function (e) {
      e.preventDefault();
      run(new FormData(el));
      location.assign(next);
    });
  }

  document.addEventListener("DOMContentLoaded", function () {
    var page = document.body.dataset.page;
    if (page === "signup") {
      var params = new URLSearchParams(location.search);
      var signup = document.getElementById("signup");
      if (params.get("plan")) signup.plan.value = params.get("plan");
      var id = Math.random().toString(36).slice(2, 8);
      signup.email.value = "sam." + id + "@example.com";
      var started = false;
      function start() {
        if (started) return;
        started = true;
        bizkit.track("signup_started", { plan: signup.plan.value });
      }
      // Safari doesn't focus a button on click, so a submit with the
      // prefilled fields may come with no focusin.
      signup.addEventListener("focusin", start);
      form("signup", "/onboarding/project", function (data) {
        start();
        var plan = data.get("plan");
        bizkit.identify("user_" + id, { email: data.get("email"), name: data.get("name"), plan: plan });
        bizkit.track("signup_completed", { plan: plan });
      });
    }
    form("project", "/onboarding/invite", function (data) {
      bizkit.track("project_created", { template: data.get("template") });
    });
    form("invite", "/onboarding/report", function (data) {
      bizkit.track("invite_sent", { role: data.get("role") });
    });
    var report = document.getElementById("open-report");
    if (report) {
      report.addEventListener("click", function () {
        bizkit.track("report_viewed", { report: "weekly" });
        location.assign("/app");
      });
    }
  });
})();
