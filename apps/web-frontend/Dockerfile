# syntax=docker/dockerfile:1.7
FROM oven/bun:1.3.14-alpine AS dependencies

WORKDIR /workspace
COPY package.json bun.lock ./
COPY apps/admin-frontend/package.json apps/admin-frontend/package.json
COPY apps/cloud-server/package.json apps/cloud-server/package.json
COPY apps/dav-bridge-desktop/package.json apps/dav-bridge-desktop/package.json
COPY apps/dav-bridge-mobile/package.json apps/dav-bridge-mobile/package.json
COPY apps/web-frontend/package.json apps/web-frontend/package.json
COPY packages/crypto-core-lib/package.json packages/crypto-core-lib/package.json
COPY tests/acceptance/package.json tests/acceptance/package.json
RUN bun install --frozen-lockfile

FROM dependencies AS runtime-dependencies
RUN rm -rf node_modules apps/*/node_modules packages/*/node_modules tests/*/node_modules \
    && bun install --frozen-lockfile --production

FROM dependencies AS builder
COPY . .
ARG VITE_KAMORI_API_BASE_URL=https://api.kamori.app
ARG VITE_KAMORI_WEB_CSRF_COOKIE_NAME=__Host-kamori_csrf
ENV VITE_KAMORI_API_BASE_URL=$VITE_KAMORI_API_BASE_URL
ENV VITE_KAMORI_WEB_CSRF_COOKIE_NAME=$VITE_KAMORI_WEB_CSRF_COOKIE_NAME
RUN bun run --filter web-frontend build

FROM node:24-alpine AS runtime
ENV NODE_ENV=production HOST=0.0.0.0 PORT=3000 ORIGIN=https://app.kamori.app
WORKDIR /workspace/apps/web-frontend
COPY --from=runtime-dependencies /workspace/node_modules /workspace/node_modules
COPY --from=runtime-dependencies /workspace/apps/web-frontend/node_modules ./node_modules
COPY --from=builder /workspace/apps/web-frontend/build ./build
USER 1000:1000
EXPOSE 3000
CMD ["node", "build/index.js"]
