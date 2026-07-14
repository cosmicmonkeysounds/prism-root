"""loom-stagehand — the show-control bridge for Loom live events.

The cue router consumes the event server's mod SSE feed and translates
story events (directives, beat entries, signals) into OSC cues for the
TouchDesigner machines and MQTT commands for props; in the other
direction it subscribes to MQTT sensor topics and injects them into the
story as journaled mod-API mutations (`signal` / `beat` / `arrive`).

Design: docs/dev/loom-show-control.md.
"""

__all__ = ["__version__"]
__version__ = "0.1.0"
