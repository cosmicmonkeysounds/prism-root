"""OSC dispatch to the TouchDesigner machines.

The `t_exec` contract: when a cue rule specifies `t_exec`, the router
appends the absolute execution time as a final **string** argument of
epoch milliseconds (a string because OSC int32 overflows epoch-ms and
float32 can't hold it; every receiver parses strings). NTP-synced
receivers apply the cue at that instant, so 8 projectors flip together.
"""

from __future__ import annotations

import logging
from typing import Any

from pythonosc.udp_client import SimpleUDPClient

log = logging.getLogger("stagehand.osc")

_INT32_MAX = 2**31 - 1
_INT32_MIN = -(2**31)


def _safe_arg(value: Any) -> Any:
    """python-osc packs int as int32; widen overflowing ints to strings."""
    if isinstance(value, bool):
        return 1 if value else 0
    if isinstance(value, int) and not (_INT32_MIN <= value <= _INT32_MAX):
        return str(value)
    if value is None:
        return ""
    return value


class OscSender:
    def __init__(self, targets: dict[str, tuple[str, int]]) -> None:
        self._clients = {name: SimpleUDPClient(host, port) for name, (host, port) in targets.items()}

    @property
    def target_names(self) -> list[str]:
        return list(self._clients)

    def send(self, addr: str, args: list[Any], to: tuple[str, ...] | None = None) -> None:
        names = to if to is not None else tuple(self._clients)
        payload = [_safe_arg(a) for a in args]
        for name in names:
            client = self._clients.get(name)
            if client is None:
                log.warning("unknown OSC target %r for %s", name, addr)
                continue
            try:
                client.send_message(addr, payload)
            except OSError as err:
                log.warning("OSC send to %s failed: %s", name, err)
