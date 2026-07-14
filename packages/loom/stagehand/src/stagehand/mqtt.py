"""The MQTT side: a paho client on its own network thread, bridged into
asyncio via a queue.

Stagehand announces itself on `health/stagehand` (retained "online",
broker-set LWT "lost") so the Run-mode tech rail can watch the bridge
the same way it watches every prop.
"""

from __future__ import annotations

import asyncio
import logging

import paho.mqtt.client as paho

log = logging.getLogger("stagehand.mqtt")

HEALTH_TOPIC = "health/stagehand"


class MqttBridge:
    def __init__(
        self,
        host: str,
        port: int,
        filters: list[str],
        loop: asyncio.AbstractEventLoop,
        client_id: str = "stagehand",
        username: str | None = None,
        password: str | None = None,
    ) -> None:
        self._filters = filters
        self._loop = loop
        self.queue: asyncio.Queue[tuple[str, bytes]] = asyncio.Queue()
        self._client = paho.Client(paho.CallbackAPIVersion.VERSION2, client_id=client_id, clean_session=True)
        if username is not None:
            self._client.username_pw_set(username, password)
        self._client.will_set(HEALTH_TOPIC, "lost", retain=True)
        self._client.on_connect = self._on_connect
        self._client.on_disconnect = self._on_disconnect
        self._client.on_message = self._on_message
        self._host = host
        self._port = port

    # paho callbacks run on the network thread — hop to the loop.

    def _on_connect(self, client: paho.Client, _userdata, _flags, reason_code, _props) -> None:
        log.info("mqtt connected (%s)", reason_code)
        client.publish(HEALTH_TOPIC, "online", retain=True)
        for f in self._filters:
            client.subscribe(f)
            log.info("mqtt subscribed %s", f)

    def _on_disconnect(self, _client, _userdata, _flags, reason_code, _props) -> None:
        log.warning("mqtt disconnected (%s) — paho will retry", reason_code)

    def _on_message(self, _client, _userdata, msg: paho.MQTTMessage) -> None:
        self._loop.call_soon_threadsafe(self.queue.put_nowait, (msg.topic, msg.payload))

    def start(self) -> None:
        self._client.connect_async(self._host, self._port)
        self._client.loop_start()

    def stop(self) -> None:
        try:
            self._client.publish(HEALTH_TOPIC, "offline", retain=True).wait_for_publish(timeout=2)
        except (RuntimeError, ValueError):
            pass
        self._client.disconnect()
        self._client.loop_stop()

    def publish(self, topic: str, payload: str, retain: bool = False, qos: int = 0) -> None:
        self._client.publish(topic, payload, qos=qos, retain=retain)
