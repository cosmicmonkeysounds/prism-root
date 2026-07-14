"""Client for the event server's mod API.

Wire contract (core/server/event-runtime.ts):
  - auth: `x-loom-token` header; bootstrap `POST /api/mod/login {passcode}`
  - `POST /api/mod/signal {name, subject?}`
  - `POST /api/mod/beat   {name, subject?}`
  - `POST /api/mod/set    {id, field: "location", value}` — the arrive path
  - SSE feed: `GET /events?role=mod&id=stagehand`

Event scoping: paths are `/e/<eventId>/...` unless the configured event
is "default"/empty, which targets the server's bare back-compat routes.
"""

from __future__ import annotations

import logging

import httpx

log = logging.getLogger("stagehand.modapi")


class ModApiError(RuntimeError):
    pass


class ModClient:
    def __init__(
        self,
        base_url: str,
        event: str = "default",
        token: str | None = None,
        passcode: str | None = None,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self.event = event
        self.token = token
        self.passcode = passcode
        self._client = httpx.AsyncClient(base_url=self.base_url, timeout=10.0)

    def path(self, p: str) -> str:
        if self.event in ("", "default"):
            return p
        return f"/e/{self.event}{p}"

    @property
    def sse_url(self) -> str:
        return f"{self.base_url}{self.path('/events')}?role=mod&id=stagehand"

    async def ensure_token(self) -> None:
        """Exchange the mod passcode for a session token if needed."""
        if self.token is not None:
            return
        if self.passcode is None:
            raise ModApiError("no mod_token and no mod_passcode configured")
        r = await self._client.post(self.path("/api/mod/login"), json={"passcode": self.passcode})
        if r.status_code != 200:
            raise ModApiError(f"mod login failed: {r.status_code} {r.text}")
        self.token = r.json()["token"]
        log.info("mod login ok (token acquired)")

    async def _post(self, p: str, body: dict) -> None:
        await self.ensure_token()
        headers = {"x-loom-token": self.token or ""}
        r = await self._client.post(self.path(p), json=body, headers=headers)
        if r.status_code == 403 and self.passcode is not None:
            # Session may have been reset server-side — re-login once.
            self.token = None
            await self.ensure_token()
            headers = {"x-loom-token": self.token or ""}
            r = await self._client.post(self.path(p), json=body, headers=headers)
        if r.status_code != 200:
            raise ModApiError(f"POST {p} → {r.status_code} {r.text}")

    async def signal(self, name: str, subject: str | None = None) -> None:
        await self._post("/api/mod/signal", {"name": name, "subject": subject or ""})

    async def beat(self, name: str, subject: str | None = None) -> None:
        await self._post("/api/mod/beat", {"name": name, "subject": subject or ""})

    async def arrive(self, person: str, location: str) -> None:
        await self._post("/api/mod/set", {"id": person, "field": "location", "value": location})

    async def aclose(self) -> None:
        await self._client.aclose()
