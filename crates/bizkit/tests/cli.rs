//! End-to-end tests that run the compiled binary.

#[cfg(test)]
mod cli {
    use std::io::{BufRead as _, BufReader, Read as _, Write as _};
    use std::net::TcpStream;
    use std::process::{Command as StdCommand, Stdio};
    use std::time::Duration;

    use assert_cmd::Command;
    use predicates::str::contains;

    fn bin() -> Command {
        let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
        cmd.env("CONFIG_FILE", "/dev/null");
        cmd
    }

    #[test]
    fn help_lists_commands() {
        bin()
            .arg("--help")
            .assert()
            .success()
            .stdout(contains("serve"))
            .stdout(contains("migrate"))
            .stdout(contains("openapi"))
            .stdout(contains("completions"));
    }

    #[test]
    fn version_prints_crate_version() {
        bin().arg("--version").assert().success().stdout(contains(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn unknown_command_fails() {
        bin().arg("not-a-command").assert().failure();
    }

    #[test]
    fn completions_generates_a_script() {
        bin().args(["completions", "bash"]).assert().success().stdout(contains("bizkit"));
    }

    #[test]
    fn openapi_prints_valid_json() {
        let output = bin().arg("openapi").assert().success().get_output().stdout.clone();
        let spec: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(spec["openapi"], "3.1.0", "an OpenAPI 3.1 document");
    }

    #[test]
    fn migrate_exits_zero() {
        let dir = tempfile::tempdir().unwrap();
        bin()
            .arg("migrate")
            .arg("--data-dir")
            .arg(dir.path())
            .assert()
            .success()
            .stdout(contains("migrated"));
    }

    #[test]
    fn serve_answers_healthz() {
        let dir = tempfile::tempdir().unwrap();
        let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin(env!("CARGO_PKG_NAME")))
            .env("CONFIG_FILE", "/dev/null")
            .args(["--no-color", "serve", "--listen", "127.0.0.1:0", "--data-dir"])
            .arg(dir.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut line = String::new();
        BufReader::new(stdout).read_line(&mut line).unwrap();
        let addr = line
            .trim()
            .strip_prefix("listening on http://")
            .unwrap_or_else(|| panic!("line: {line}"))
            .to_owned();

        let mut stream = TcpStream::connect(&addr).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        write!(stream, "GET /healthz HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "healthz response: {response}");
        assert!(response.ends_with("ok"), "healthz body: {response}");
    }

    #[cfg(all(feature = "test-hooks", feature = "analytics"))]
    #[test]
    fn dev_demo_site_needs_the_demo_seed() {
        let dir = tempfile::tempdir().unwrap();
        bin()
            .args(["dev", "demo-site", "--listen", "127.0.0.1:0"])
            .args(["--snippet-base", "http://localhost:5173", "--data-dir"])
            .arg(dir.path())
            .assert()
            .failure()
            .stderr(contains("run `bizkit dev seed-demo` first"));
    }

    #[cfg(feature = "test-hooks")]
    #[test]
    fn dev_seed_demo_runs_twice() {
        let dir = tempfile::tempdir().unwrap();
        for _ in 0..2 {
            bin()
                .args(["dev", "seed-demo", "--data-dir"])
                .arg(dir.path())
                .assert()
                .success()
                .stdout(contains("sign in as demo@bizkit.test"));
        }
    }
}
