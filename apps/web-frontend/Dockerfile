# syntax=docker/dockerfile:1.7
FROM oven/bun:1.3.14-alpine AS builder

WORKDIR /workspace
COPY . .
RUN bun install --frozen-lockfile
ARG VITE_KAMORI_API_BASE_URL=https://api.kamori.app
ARG VITE_KAMORI_WEB_CSRF_COOKIE_NAME=__Host-kamori_csrf
ENV VITE_KAMORI_API_BASE_URL=$VITE_KAMORI_API_BASE_URL
ENV VITE_KAMORI_WEB_CSRF_COOKIE_NAME=$VITE_KAMORI_WEB_CSRF_COOKIE_NAME
RUN bun run --filter web-frontend build

FROM node:24-alpine AS runtime
ENV NODE_ENV=production HOST=0.0.0.0 PORT=3000 ORIGIN=https://app.kamori.app
WORKDIR /app
COPY --from=builder /workspace/apps/web-frontend/build ./build
COPY --from=builder /workspace/node_modules ./node_modules
USER 1000:1000
EXPOSE 3000
CMD ["node", "build/index.js"]
