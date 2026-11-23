//! The bench end to end without Docker: a server in-process, a short load,
//! and every acked event stored after a clean shutdown.

#[cfg(test)]
mod smoke {
    use std::collections::HashSet;

    use bizkit_bench::load::{self, LoadArgs};
    use bizkit_bench::pipeline::Design;
    use bizkit_bench::report::{self, Labels};
    use bizkit_bench::serve::{self, ServeArgs};
    use bizkit_bench::store::{self, Layout, Sync};

    async fn smoke(design: Design) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let serve_args = ServeArgs {
            data: data.clone(),
            design,
            layout: Layout::PerProject,
            sync: Sync::Normal,
            flush_ms: 100,
            flush_max: 1000,
            queue: 10_000,
            listen: "127.0.0.1:0".parse().unwrap(),
        };
        let server = serve::start(&serve_args).await.unwrap();
        let out = dir.path().join("run.json");
        let load_args = LoadArgs {
            url: format!("http://{}", server.addr),
            rate: 300,
            projects: 3,
            duration: 2,
            hot: 0,
            burst_x: 1,
            burst_at: 0,
            burst_for: 0,
            queries_per_s: 5,
            app_per_s: 5,
            rollup_every: 1,
            max_inflight: 512,
            out: out.clone(),
            labels: Labels {
                scenario: "smoke".to_owned(),
                design,
                layout: Layout::PerProject,
                sync: Sync::Normal,
                flush_ms: 100,
            },
        };
        let result = load::run(&load_args).await.unwrap();
        server.shutdown().await;
        assert!(result.acked > 500, "{design:?}: {result:?}");
        assert_eq!(result.acked, result.sent, "{design:?}: every send acked");
        assert!(result.query_ms.is_some(), "queries ran");
        assert!(!result.rollup_ms.is_empty(), "rollups ran");
        let acked = std::fs::read_to_string(out.with_extension("acked")).unwrap();
        let stored: HashSet<u64> = store::stored_seqs(&data).await.unwrap().into_iter().collect();
        assert_eq!(report::lost(&acked, &stored), 0, "{design:?}: nothing lost on a clean stop");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn every_design_serves_a_short_load() {
        for design in [Design::WriteThrough, Design::Batched, Design::Buffered] {
            smoke(design).await;
        }
    }
}
