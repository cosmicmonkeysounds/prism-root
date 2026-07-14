"""Minimal Server-Sent-Events line-protocol parsing.

The event server frames as `event: <name>\\ndata: <json>\\n\\n` with
`:ping` comment keepalives (`http-util.ts::sseSend`). The parser is
incremental — feed it arbitrary chunks, get complete frames back.
"""

from __future__ import annotations


class SSEParser:
    def __init__(self) -> None:
        self._buf = ""

    def feed(self, chunk: str) -> list[tuple[str, str]]:
        """Append a chunk; return every complete (event, data) frame."""
        self._buf = (self._buf + chunk).replace("\r\n", "\n")
        frames: list[tuple[str, str]] = []
        while (end := self._buf.find("\n\n")) != -1:
            block = self._buf[:end]
            self._buf = self._buf[end + 2 :]
            event = "message"
            datas: list[str] = []
            for line in block.split("\n"):
                if line.startswith(":") or line == "":
                    continue
                if line.startswith("event:"):
                    event = line[len("event:") :].strip()
                elif line.startswith("data:"):
                    datas.append(line[len("data:") :].lstrip())
            if datas:
                frames.append((event, "\n".join(datas)))
        return frames
