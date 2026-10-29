# The user manual as a static site (Railway). Built from the repository root so pnpm sees the
# workspace lockfile: `docker build -f site/Dockerfile .`
FROM node:22-alpine AS build
ENV ASTRO_TELEMETRY_DISABLED=1
WORKDIR /repo
RUN corepack enable
# Only the manifests first, so the dependency layer is cached between content edits.
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY patches ./patches
COPY ui/package.json ./ui/
COPY site/package.json ./site/
RUN pnpm install --frozen-lockfile --filter reestr-site
COPY site ./site
RUN pnpm --filter reestr-site build

FROM caddy:2-alpine
COPY site/Caddyfile /etc/caddy/Caddyfile
COPY --from=build /repo/site/dist /srv
