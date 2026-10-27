# Runtime image for one mazbri service. The binary is compiled natively per
# architecture in CI (release.yml) and copied in from the build context:
#   <context>/<arch>/<BIN>   arch = amd64 | arm64 (buildx TARGETARCH)
# Binaries need GLIBC >= 2.38, so the base is debian trixie (bookworm is 2.36).
#
# Runtime contract:
#   - bootstrap is NOT baked in; mount it at /etc/mazbri/<SVC>-bootstrap.toml
#   - logs: set bootstrap [tracing] log_dir = "/logs" (absolute) and mount a
#     volume at /logs; set LOG_INSTANCE_ID per instance. stdout is always on.
#   - 主令牌只给 cert/mgmt，其余服务注入派生令牌（容器内变量名仍是 MAZBRI_CERT_SERVICE_TOKEN）。
FROM debian:trixie-slim

ARG BIN
ARG SVC
ARG TARGETARCH

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 mazbri \
    && useradd --system --uid 10001 --gid mazbri --no-create-home --shell /usr/sbin/nologin mazbri \
    && mkdir -p /etc/mazbri /logs \
    && chown mazbri:mazbri /logs

ENV TZ=Asia/Shanghai \
    MAZBRI_BIN=${BIN} \
    MAZBRI_BOOTSTRAP=/etc/mazbri/${SVC}-bootstrap.toml

COPY --chmod=0755 ${TARGETARCH}/${BIN} /usr/local/bin/${BIN}

USER 10001:10001
WORKDIR /logs
VOLUME ["/logs"]

# exec form cannot expand ARGs, so a tiny sh wrapper resolves the fixed path;
# extra container args (e.g. --dev for mgmt) are appended after --bootstrap.
ENTRYPOINT ["/bin/sh", "-c", "exec /usr/local/bin/\"$MAZBRI_BIN\" --bootstrap \"$MAZBRI_BOOTSTRAP\" \"$@\"", "--"]
