# ── Build stage ──────────────────────────────────────────────
FROM node:22-slim AS build
RUN corepack enable && corepack prepare pnpm@9.15.4 --activate
WORKDIR /app

# Install dependencies (layer-cached when lock/manifests unchanged)
COPY pnpm-workspace.yaml package.json pnpm-lock.yaml ./
COPY packages/shared/package.json packages/shared/
COPY packages/prism-core/package.json packages/prism-core/
COPY packages/prism-relay/package.json packages/prism-relay/
RUN pnpm install --frozen-lockfile

# Copy source (shared + core are consumed as TS source via path aliases)
COPY tsconfig.base.json ./
COPY packages/shared/ packages/shared/
COPY packages/prism-core/ packages/prism-core/
COPY packages/prism-relay/ packages/prism-relay/

# Build relay (tsc emits to dist/)
RUN pnpm --filter @prism/relay build

# Prune dev dependencies for production image
RUN pnpm prune --prod

# ── Production stage ─────────────────────────────────────────
FROM node:22-slim AS production
RUN corepack enable && corepack prepare pnpm@9.15.4 --activate
WORKDIR /app

# Create non-root user
RUN groupadd --gid 1001 prism && \
    useradd --uid 1001 --gid prism --create-home prism && \
    mkdir -p /home/prism/.prism/relay && \
    chown -R prism:prism /home/prism/.prism

# Copy built artifacts and production dependencies
COPY --from=build /app/package.json ./
COPY --from=build /app/pnpm-workspace.yaml ./
COPY --from=build /app/node_modules/ node_modules/
COPY --from=build /app/packages/shared/ packages/shared/
COPY --from=build /app/packages/prism-core/ packages/prism-core/
COPY --from=build /app/packages/prism-relay/dist/ packages/prism-relay/dist/
COPY --from=build /app/packages/prism-relay/package.json packages/prism-relay/
COPY --from=build /app/packages/prism-relay/node_modules/ packages/prism-relay/node_modules/

ENV PRISM_RELAY_MODE=server
ENV PRISM_RELAY_HOST=0.0.0.0
ENV PRISM_RELAY_PORT=4444
ENV PRISM_RELAY_DATA_DIR=/home/prism/.prism/relay

EXPOSE 4444
VOLUME ["/home/prism/.prism/relay"]

USER prism

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD node -e "fetch('http://localhost:4444/api/health').then(r=>{if(!r.ok)process.exit(1)}).catch(()=>process.exit(1))"

CMD ["node", "packages/prism-relay/dist/cli.js"]
