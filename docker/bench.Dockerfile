# The bizkit-bench server image. The binary is a static musl build made on
# the host by scripts/bench-ingest.sh; the build context is its directory.
FROM alpine:3.24
COPY bizkit-bench /usr/local/bin/bizkit-bench
ENTRYPOINT ["/usr/local/bin/bizkit-bench"]
