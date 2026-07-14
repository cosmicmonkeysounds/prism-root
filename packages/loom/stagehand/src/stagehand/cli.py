"""`stagehand run --config show.yaml` / `stagehand check --config show.yaml`."""

from __future__ import annotations

import argparse
import asyncio
import logging
import sys

from .config import ConfigError, StagehandConfig, load_config
from .router import Router


def _describe(cfg: StagehandConfig) -> str:
    lines = [
        f"server   : {cfg.server.url} (event: {cfg.server.event}, auth: {'token' if cfg.server.mod_token else 'passcode'})",
        f"mqtt     : {f'{cfg.mqtt.host}:{cfg.mqtt.port}' if cfg.mqtt else '— (no broker)'}",
        f"osc      : {', '.join(f'{n}={h}:{p}' for n, (h, p) in cfg.osc_targets.items()) or '— (no targets)'}",
        f"cues     : {len(cfg.cues)} rule(s)",
        f"sensors  : {len(cfg.sensors)} rule(s)",
    ]
    if cfg.mqtt_filters:
        lines.append(f"subscribe: {', '.join(cfg.mqtt_filters)}")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="stagehand", description="Loom show-control bridge")
    parser.add_argument("-v", "--verbose", action="store_true", help="debug logging")
    sub = parser.add_subparsers(dest="command", required=True)
    for name, help_text in (("run", "run the cue router"), ("check", "validate a config and exit")):
        p = sub.add_parser(name, help=help_text)
        p.add_argument("--config", required=True, help="path to show.yaml")
    args = parser.parse_args(argv)

    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s %(name)s %(levelname)s %(message)s",
        datefmt="%H:%M:%S",
    )

    try:
        cfg = load_config(args.config)
    except (ConfigError, ValueError) as err:
        print(f"config error: {err}", file=sys.stderr)
        return 2

    print(_describe(cfg))
    if args.command == "check":
        print("config OK")
        return 0

    try:
        asyncio.run(Router(cfg).run())
    except KeyboardInterrupt:
        print("stagehand: stopped")
    return 0


if __name__ == "__main__":  # pragma: no cover
    sys.exit(main())
