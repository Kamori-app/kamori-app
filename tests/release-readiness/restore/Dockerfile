FROM quay.io/minio/mc:RELEASE.2025-08-13T08-35-41Z@sha256:a7fe349ef4bd8521fb8497f55c6042871b2ae640607cf99d9bede5e9bdf11727 AS mc
FROM docker.io/library/ubuntu:24.04@sha256:33ceb71981b602c1a7443a53469e4dba065f7503eab3078a2d7a57a2ab987517
LABEL kamori.readiness="20261015" kamori.readiness.task="restore"
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends postgresql-16=16.15-0ubuntu0.24.04.1 pgbackrest=2.50-1build2 rclone=1.60.1+dfsg-3ubuntu0.24.04.6 python3=3.12.3-0ubuntu2.1 openssl=3.0.13-0ubuntu3.16 ca-certificates=20260601~24.04.1 shellcheck=0.9.0-1 && rm -rf /var/lib/apt/lists/*
COPY --from=mc /usr/bin/mc /usr/local/bin/mc
ENV PATH="/usr/lib/postgresql/16/bin:${PATH}"
COPY pg-setup.sh seed.sql blob-exercise.sh /exercise/
CMD ["sleep", "infinity"]
