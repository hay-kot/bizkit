# GoReleaser builds static musl binaries outside Docker and places them at
# <os>/<arch>/bizkit in the build context.
FROM gcr.io/distroless/static-debian12:nonroot
ARG TARGETPLATFORM
COPY ${TARGETPLATFORM}/bizkit /usr/local/bin/bizkit
ENTRYPOINT ["/usr/local/bin/bizkit"]
