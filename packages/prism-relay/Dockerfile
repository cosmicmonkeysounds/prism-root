FROM node:22-slim AS base
RUN corepack enable && corepack prepare pnpm@9.15.4 --activate
WORKDIR /app

# Install dependencies
COPY pnpm-workspace.yaml package.json pnpm-lock.yaml ./
COPY packages/shared/package.json packages/shared/
COPY packages/prism-core/package.json packages/prism-core/
COPY packages/prism-relay/package.json packages/prism-relay/
RUN pnpm install --frozen-lockfile --prod=false

# Copy source
COPY packages/shared/ packages/shared/
COPY packages/prism-core/ packages/prism-core/
COPY packages/prism-relay/ packages/prism-relay/
COPY tsconfig.base.json ./

# Build
RUN pnpm --filter @prism/relay build

# Production stage
FROM node:22-slim AS production
RUN corepack enable && corepack prepare pnpm@9.15.4 --activate
WORKDIR /app

COPY --from=base /app/packages/prism-relay/dist/ packages/prism-relay/dist/
COPY --from=base /app/packages/prism-relay/package.json packages/prism-relay/
COPY --from=base /app/node_modules/ node_modules/
COPY --from=base /app/packages/prism-core/ packages/prism-core/
COPY --from=base /app/packages/shared/ packages/shared/
COPY --from=base /app/package.json ./

ENV PRISM_RELAY_MODE=server
ENV PRISM_RELAY_HOST=0.0.0.0
ENV PRISM_RELAY_PORT=4444
EXPOSE 4444

CMD ["node", "packages/prism-relay/dist/cli.js"]
