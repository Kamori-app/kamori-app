# syntax=docker/dockerfile:1.7
FROM caddy:2.11.4-alpine
RUN setcap -r /usr/bin/caddy \
    && test -z "$(getcap /usr/bin/caddy)"
COPY deploy/edge/Caddyfile /etc/caddy/Caddyfile
