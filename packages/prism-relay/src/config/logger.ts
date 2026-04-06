/**
 * Structured logger for Prism Relay.
 * Outputs text or JSON depending on config.
 */

export type LogLevel = "debug" | "info" | "warn" | "error";

const LEVEL_ORDER: Record<LogLevel, number> = {
  debug: 0,
  info: 1,
  warn: 2,
  error: 3,
};

export interface RelayLogger {
  debug(msg: string, data?: Record<string, unknown>): void;
  info(msg: string, data?: Record<string, unknown>): void;
  warn(msg: string, data?: Record<string, unknown>): void;
  error(msg: string, data?: Record<string, unknown>): void;
}

export function createLogger(options: {
  level: LogLevel;
  format: "text" | "json";
}): RelayLogger {
  const minLevel = LEVEL_ORDER[options.level];

  function shouldLog(level: LogLevel): boolean {
    return LEVEL_ORDER[level] >= minLevel;
  }

  function formatText(level: LogLevel, msg: string, data?: Record<string, unknown>): string {
    const ts = new Date().toISOString();
    const prefix = `${ts} [${level.toUpperCase().padEnd(5)}]`;
    if (data && Object.keys(data).length > 0) {
      const pairs = Object.entries(data).map(([k, v]) => `${k}=${JSON.stringify(v)}`).join(" ");
      return `${prefix} ${msg} ${pairs}`;
    }
    return `${prefix} ${msg}`;
  }

  function formatJson(level: LogLevel, msg: string, data?: Record<string, unknown>): string {
    return JSON.stringify({
      ts: new Date().toISOString(),
      level,
      msg,
      ...data,
    });
  }

  const fmt = options.format === "json" ? formatJson : formatText;
  const out = (s: string) => process.stdout.write(s + "\n");
  const err = (s: string) => process.stderr.write(s + "\n");

  return {
    debug(msg, data) {
      if (shouldLog("debug")) out(fmt("debug", msg, data));
    },
    info(msg, data) {
      if (shouldLog("info")) out(fmt("info", msg, data));
    },
    warn(msg, data) {
      if (shouldLog("warn")) err(fmt("warn", msg, data));
    },
    error(msg, data) {
      if (shouldLog("error")) err(fmt("error", msg, data));
    },
  };
}
